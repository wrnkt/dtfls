//! `config.json` at the repo root: loading, defaults and the `track` list.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map, Value};

pub const CONFIG_NAME: &str = "config.json";

pub const DEFAULT_IGNORE: &[&str] = &[
    ".DS_Store",
    "*.swp",
    "*.swo",
    "*.orig",
    ".git",
    CONFIG_NAME,
    "README*",
    "LICENSE*",
    ".gitignore",
];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Hooks {
    #[serde(default)]
    pub pre_sync: Vec<String>,
    #[serde(default)]
    pub post_sync: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Mapping {
    pub src: String,
    pub dest: String,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub distro: Option<String>,
}

/// One `track` entry: a plain path, or an object with filters / overrides.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TrackEntry {
    Path(String),
    Detailed {
        path: String,
        #[serde(default)]
        os: Option<String>,
        #[serde(default)]
        distro: Option<String>,
        #[serde(default)]
        src: Option<String>,
        #[serde(default)]
        dest: Option<String>,
    },
}

impl TrackEntry {
    pub fn path(&self) -> &str {
        match self {
            TrackEntry::Path(p) => p,
            TrackEntry::Detailed { path, .. } => path,
        }
    }
}

/// Effective config: user settings merged over defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub ignore: Vec<String>,
    pub hooks: Hooks,
    /// `None` means "use the home/ home.<os>/ home.<distro>/ convention".
    pub mappings: Option<Vec<Mapping>>,
    pub track: Vec<TrackEntry>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ignore: DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect(),
            hooks: Hooks::default(),
            mappings: None,
            track: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
struct UserConfig {
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    hooks: Option<Hooks>,
    #[serde(default)]
    mappings: Option<Vec<Mapping>>,
    #[serde(default)]
    track: Vec<TrackEntry>,
}

/// Raw user config as a JSON object (`{}` if the file is absent). Kept
/// untyped so rewriting it preserves keys and ordering we don't touch.
pub fn load_user(repo: &Path) -> Result<Map<String, Value>> {
    let path = repo.join(CONFIG_NAME);
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    match serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))? {
        Value::Object(map) => Ok(map),
        _ => anyhow::bail!("{} must contain a JSON object", path.display()),
    }
}

pub fn save_user(repo: &Path, cfg: &Map<String, Value>) -> Result<()> {
    let path = repo.join(CONFIG_NAME);
    let mut text = serde_json::to_string_pretty(cfg)?;
    text.push('\n');
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

/// Create `config.json` with empty defaults if it does not exist.
pub fn ensure(repo: &Path) -> Result<()> {
    if !repo.join(CONFIG_NAME).exists() {
        save_user(repo, &Map::new())?;
    }
    Ok(())
}

pub fn load(repo: &Path) -> Result<Config> {
    let raw = load_user(repo)?;
    let user: UserConfig = serde_json::from_value(Value::Object(raw))
        .with_context(|| format!("invalid {}", repo.join(CONFIG_NAME).display()))?;

    let mut cfg = Config::default();
    for pat in user.ignore {
        if !cfg.ignore.contains(&pat) {
            cfg.ignore.push(pat);
        }
    }
    if let Some(hooks) = user.hooks {
        cfg.hooks = hooks;
    }
    cfg.mappings = user.mappings;
    cfg.track = user.track;
    Ok(cfg)
}

/// Append path strings to the `track` list in `config.json`, skipping any
/// already present (as a plain string or as an object's `path`).
pub fn add_to_track(repo: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut user = load_user(repo)?;
    let slot = user
        .entry("track")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !slot.is_array() {
        *slot = Value::Array(Vec::new());
    }
    let track = slot.as_array_mut().expect("just ensured an array");
    let mut changed = false;
    for p in paths {
        let present = track.iter().any(|e| match e {
            Value::String(s) => s == p,
            Value::Object(o) => o.get("path").and_then(Value::as_str) == Some(p),
            _ => false,
        });
        if !present {
            track.push(Value::String(p.clone()));
            changed = true;
        }
    }
    if changed {
        save_user(repo, &user)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repo_with(cfg: Option<Value>) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        if let Some(v) = cfg {
            fs::write(tmp.path().join(CONFIG_NAME), v.to_string()).unwrap();
        }
        tmp
    }

    #[test]
    fn defaults_when_no_file() {
        let repo = repo_with(None);
        let cfg = load(repo.path()).unwrap();
        assert_eq!(cfg, Config::default());
        assert!(cfg.mappings.is_none());
        assert!(cfg.track.is_empty());
    }

    #[test]
    fn merges_ignore_patterns_without_duplicates() {
        let repo = repo_with(Some(json!({"ignore": ["secrets/", "*.swp"]})));
        let cfg = load(repo.path()).unwrap();
        assert!(cfg.ignore.contains(&"secrets/".to_string()));
        assert_eq!(cfg.ignore.iter().filter(|p| *p == "*.swp").count(), 1);
    }

    #[test]
    fn passes_through_track_mappings_and_hooks() {
        let repo = repo_with(Some(json!({
            "track": ["~/.bashrc", {"path": "~/.zprofile", "os": "darwin"}],
            "mappings": [{"src": "home", "dest": "~"}],
            "hooks": {"pre_sync": ["echo hi"]}
        })));
        let cfg = load(repo.path()).unwrap();
        assert_eq!(cfg.track[0], TrackEntry::Path("~/.bashrc".into()));
        assert_eq!(cfg.track[1].path(), "~/.zprofile");
        assert_eq!(cfg.mappings.unwrap()[0].src, "home");
        assert_eq!(cfg.hooks.pre_sync, vec!["echo hi"]);
        assert!(cfg.hooks.post_sync.is_empty());
    }

    #[test]
    fn empty_mappings_list_is_still_explicit() {
        let repo = repo_with(Some(json!({"mappings": []})));
        assert_eq!(load(repo.path()).unwrap().mappings, Some(vec![]));
    }

    #[test]
    fn add_to_track_dedupes_against_strings_and_objects() {
        let repo = repo_with(Some(json!({
            "ignore": ["x"],
            "track": [{"path": "~/.zprofile", "os": "darwin"}, "~/.bashrc"]
        })));
        add_to_track(
            repo.path(),
            &["~/.bashrc".into(), "~/.zprofile".into(), "~/.vimrc".into()],
        )
        .unwrap();
        let user = load_user(repo.path()).unwrap();
        assert_eq!(
            user["track"],
            json!([{"path": "~/.zprofile", "os": "darwin"}, "~/.bashrc", "~/.vimrc"])
        );
        // Untouched keys keep their position.
        assert_eq!(user.keys().next().unwrap(), "ignore");
    }

    #[test]
    fn ensure_creates_empty_object() {
        let repo = repo_with(None);
        ensure(repo.path()).unwrap();
        assert_eq!(
            fs::read_to_string(repo.path().join(CONFIG_NAME)).unwrap(),
            "{}\n"
        );
    }
}
