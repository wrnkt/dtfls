//! Which repo directories map to which system directories, and where a given
//! system file lives in the repo.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::{Config, TrackEntry};
use crate::ctx::Ctx;
use crate::fsutil::{expand_tilde, relative_to, resolve_lenient};
use crate::git::Git;
use crate::ui;

/// A repo directory (`"home"`, `"home.linux"`, or `"."` for the repo root)
/// and the system directory it deploys to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcDir {
    pub name: String,
    pub dest: PathBuf,
}

impl SrcDir {
    pub fn new(name: &str, dest: PathBuf) -> Self {
        SrcDir {
            name: name.to_string(),
            dest,
        }
    }

    /// `home/`, or `(repo root)` for `"."`.
    pub fn label(&self) -> String {
        if self.name == "." {
            "(repo root)".into()
        } else {
            format!("{}/", self.name)
        }
    }

    /// Absolute path of this directory inside the repo.
    pub fn root(&self, repo: &Path) -> PathBuf {
        if self.name == "." {
            repo.to_path_buf()
        } else {
            repo.join(&self.name)
        }
    }
}

/// `src/rel` for display, or just `rel` for the repo root.
pub fn display_in_src(src: &str, rel: &Path) -> String {
    if src == "." {
        rel.display().to_string()
    } else {
        format!("{src}/{}", rel.display())
    }
}

/// Source dirs active on this platform, ordered general → specific
/// (`home` → `home.linux` → `home.fedora`).
pub fn active_dirs(ctx: &Ctx, config: &Config) -> Vec<SrcDir> {
    let mut active = Vec::new();
    match &config.mappings {
        Some(mappings) => {
            for m in mappings {
                if !ctx.platform.matches(m.os.as_deref(), m.distro.as_deref()) {
                    continue;
                }
                if ctx.repo.join(&m.src).is_dir() {
                    let dest = resolve_lenient(&expand_tilde(&m.dest, &ctx.home));
                    active.push(SrcDir::new(&m.src, dest));
                }
            }
        }
        None => {
            let mut candidates = vec!["home".to_string(), format!("home.{}", ctx.platform.os)];
            if let Some(d) = &ctx.platform.distro {
                candidates.push(format!("home.{d}"));
            }
            for name in candidates {
                if ctx.repo.join(&name).is_dir() {
                    active.push(SrcDir::new(&name, ctx.home.clone()));
                }
            }
            // No home/ subdirs found — treat the repo root itself as $HOME.
            if active.is_empty() {
                active.push(SrcDir::new(".", ctx.home.clone()));
            }
        }
    }
    active
}

/// The most specific dir in *dirs* whose destination contains *path*.
pub fn route<'a>(path: &Path, dirs: &'a [SrcDir]) -> Option<&'a SrcDir> {
    dirs.iter().rev().find(|d| path.starts_with(&d.dest))
}

/// A `track` entry resolved for this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracked {
    /// Where the file lives on the system.
    pub deployed: PathBuf,
    /// Repo directory it belongs to.
    pub src: String,
    /// Path relative to that directory.
    pub rel: PathBuf,
    /// Absolute path inside the repo.
    pub repo_path: PathBuf,
}

impl Tracked {
    pub fn label(&self) -> String {
        display_in_src(&self.src, &self.rel)
    }
}

/// Expand `track` into concrete files active on this machine. Entries without
/// an explicit `src` go to the most specific active dir that covers them.
/// The files don't need to exist yet.
pub fn resolve_tracked_files(ctx: &Ctx, config: &Config) -> Vec<Tracked> {
    let dirs = active_dirs(ctx, config);
    let mut result = Vec::new();

    for entry in &config.track {
        let (os, distro, src, dest) = match entry {
            TrackEntry::Path(_) => (None, None, None, None),
            TrackEntry::Detailed {
                os,
                distro,
                src,
                dest,
                ..
            } => (
                os.as_deref(),
                distro.as_deref(),
                src.as_deref(),
                dest.as_deref(),
            ),
        };
        if !ctx.platform.matches(os, distro) {
            continue;
        }
        let deployed = ctx.resolve(entry.path());

        let (src_name, dest_dir) = match src {
            // Explicit src bucket override.
            Some(src) => (
                src.to_string(),
                resolve_lenient(&expand_tilde(dest.unwrap_or("~"), &ctx.home)),
            ),
            None => match route(&deployed, &dirs) {
                Some(d) => (d.name.clone(), d.dest.clone()),
                None => {
                    ui::warn(&format!(
                        "Cannot route {} to any active src dir — skipping",
                        entry.path()
                    ));
                    continue;
                }
            },
        };

        let Some(rel) = relative_to(&deployed, &dest_dir) else {
            ui::warn(&format!(
                "{} is not under {} — skipping",
                entry.path(),
                dest_dir.display()
            ));
            continue;
        };
        let repo_path = SrcDir::new(&src_name, dest_dir).root(&ctx.repo).join(&rel);
        result.push(Tracked {
            deployed,
            src: src_name,
            rel,
            repo_path,
        });
    }
    result
}

/// Ignore patterns, matched (like `fnmatch`) against every path component.
/// A trailing `/` (e.g. `"secrets/"`) matches a directory of that name.
pub struct Ignore(GlobSet);

impl Ignore {
    pub fn new(patterns: &[String]) -> Self {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            let p = p.trim_end_matches('/');
            match Glob::new(p) {
                Ok(g) => {
                    b.add(g);
                }
                Err(e) => ui::warn(&format!("Invalid ignore pattern {p:?}: {e}")),
            }
        }
        Ignore(b.build().unwrap_or_else(|_| GlobSet::empty()))
    }

    pub fn is_ignored(&self, rel: &Path) -> bool {
        rel.components().any(|c| self.0.is_match(c.as_os_str()))
    }
}

/// A file in a branch under a source dir.
pub struct BranchFile {
    /// Path relative to the repo root (for `git show`).
    pub repo_rel: String,
    /// Path relative to the source dir (= relative to its destination).
    pub rel: PathBuf,
}

/// All non-ignored files in *branch* under *src*, via `git ls-tree` — no
/// checkout needed.
pub fn branch_files(git: &Git, branch: &str, src: &str, ignore: &Ignore) -> Vec<BranchFile> {
    let prefix = if src == "." {
        ".".to_string()
    } else {
        format!("{src}/")
    };
    git.ls_files(branch, &prefix)
        .into_iter()
        .filter_map(|repo_rel| {
            let p = PathBuf::from(&repo_rel);
            if ignore.is_ignored(&p) {
                return None;
            }
            let rel = if src == "." {
                p
            } else {
                p.strip_prefix(src).ok()?.to_path_buf()
            };
            Some(BranchFile { repo_rel, rel })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Mapping;
    use crate::platform::Platform;
    use std::fs;

    struct Fixture {
        _tmp: tempfile::TempDir,
        ctx: Ctx,
    }

    fn fixture(os: &str, distro: Option<&str>) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let repo = base.join("repo");
        let home = base.join("home");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
        let ctx = Ctx::new(repo, home, Platform::new(os, distro), "box".into());
        Fixture { _tmp: tmp, ctx }
    }

    fn names(dirs: &[SrcDir]) -> Vec<&str> {
        dirs.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn falls_back_to_repo_root_when_no_home_dirs() {
        let f = fixture("linux", Some("fedora"));
        let dirs = active_dirs(&f.ctx, &Config::default());
        assert_eq!(dirs, vec![SrcDir::new(".", f.ctx.home.clone())]);
    }

    #[test]
    fn picks_most_specific_active_dir_order() {
        let f = fixture("linux", Some("fedora"));
        for d in ["home", "home.linux", "home.fedora", "home.darwin"] {
            fs::create_dir(f.ctx.repo.join(d)).unwrap();
        }
        let dirs = active_dirs(&f.ctx, &Config::default());
        assert_eq!(names(&dirs), ["home", "home.linux", "home.fedora"]);
    }

    #[test]
    fn explicit_mappings_filter_by_os_and_distro() {
        let f = fixture("darwin", None);
        fs::create_dir(f.ctx.repo.join("home")).unwrap();
        fs::create_dir(f.ctx.repo.join("home.fedora-only")).unwrap();
        let cfg = Config {
            mappings: Some(vec![
                Mapping {
                    src: "home".into(),
                    dest: "~".into(),
                    os: None,
                    distro: None,
                },
                Mapping {
                    src: "home.fedora-only".into(),
                    dest: "~".into(),
                    os: Some("linux".into()),
                    distro: Some("fedora".into()),
                },
            ]),
            ..Config::default()
        };
        assert_eq!(names(&active_dirs(&f.ctx, &cfg)), ["home"]);
    }

    #[test]
    fn route_picks_most_specific() {
        let home = PathBuf::from("/h");
        let dirs = vec![
            SrcDir::new("home", home.clone()),
            SrcDir::new("home.linux", home.clone()),
        ];
        assert_eq!(
            route(&home.join(".bashrc"), &dirs).unwrap().name,
            "home.linux"
        );
        assert!(route(Path::new("/elsewhere"), &dirs).is_none());
        // Component-wise: /hx is not under /h.
        assert!(route(Path::new("/hx/file"), &dirs).is_none());
    }

    #[test]
    fn routes_tracked_file_to_active_home_dir() {
        let f = fixture("linux", Some("fedora"));
        fs::create_dir(f.ctx.repo.join("home")).unwrap();
        let cfg = Config {
            track: vec![TrackEntry::Path("~/.bashrc".into())],
            ..Config::default()
        };
        let got = resolve_tracked_files(&f.ctx, &cfg);
        assert_eq!(
            got,
            vec![Tracked {
                deployed: f.ctx.home.join(".bashrc"),
                src: "home".into(),
                rel: ".bashrc".into(),
                repo_path: f.ctx.repo.join("home/.bashrc"),
            }]
        );
    }

    #[test]
    fn explicit_src_override_bypasses_routing() {
        let f = fixture("linux", Some("fedora"));
        let cfg = Config {
            track: vec![TrackEntry::Detailed {
                path: "~/.bashrc".into(),
                os: None,
                distro: None,
                src: Some("home.linux".into()),
                dest: None,
            }],
            ..Config::default()
        };
        let got = resolve_tracked_files(&f.ctx, &cfg);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].src, "home.linux");
        assert_eq!(got[0].repo_path, f.ctx.repo.join("home.linux/.bashrc"));
    }

    #[test]
    fn os_filtered_entries_are_skipped() {
        let f = fixture("darwin", None);
        let cfg = Config {
            track: vec![TrackEntry::Detailed {
                path: "~/.bash_profile".into(),
                os: Some("linux".into()),
                distro: None,
                src: None,
                dest: None,
            }],
            ..Config::default()
        };
        assert!(resolve_tracked_files(&f.ctx, &cfg).is_empty());
    }

    #[test]
    fn repo_root_bucket_maps_into_repo_root() {
        let f = fixture("linux", None);
        let cfg = Config {
            track: vec![TrackEntry::Path("~/.vimrc".into())],
            ..Config::default()
        };
        let got = resolve_tracked_files(&f.ctx, &cfg);
        assert_eq!(got[0].repo_path, f.ctx.repo.join(".vimrc"));
        assert_eq!(got[0].label(), ".vimrc");
    }

    #[test]
    fn ignore_matches_any_component_and_dir_patterns() {
        let ig = Ignore::new(&["*.swp".into(), "secrets/".into(), "README*".into()]);
        assert!(ig.is_ignored(Path::new("home/.vim/x.swp")));
        assert!(ig.is_ignored(Path::new("home/secrets/token")));
        assert!(ig.is_ignored(Path::new("README.md")));
        assert!(!ig.is_ignored(Path::new("home/.bashrc")));
        assert!(!ig.is_ignored(Path::new("home/mysecrets")));
    }
}
