//! Everything a command needs to know about where it is running. Replaces
//! the Python module's global `REPO` so tests can build one directly.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fsutil::{expand_tilde, resolve_lenient};
use crate::git::{self, Git};
use crate::platform::{self, Platform};
use crate::ui;

/// Used unless `$DTFLS_REPO` overrides it.
pub const DEFAULT_REPO_DIR: &str = "~/.dtfls";

pub struct Ctx {
    pub repo: PathBuf,
    pub home: PathBuf,
    pub platform: Platform,
    pub hostname: String,
}

impl Ctx {
    pub fn new(repo: PathBuf, home: PathBuf, platform: Platform, hostname: String) -> Self {
        Ctx {
            repo,
            home,
            platform,
            hostname,
        }
    }

    /// Context for the real machine: `$HOME`, detected platform, and the repo
    /// resolved (and created on first use) by [`resolve_repo`].
    pub fn from_env() -> Result<Self> {
        let home = home_dir()?;
        let repo = resolve_repo(&home)?;
        Ok(Ctx::new(
            repo,
            home,
            Platform::detect(),
            platform::hostname(),
        ))
    }

    pub fn git(&self) -> Git {
        Git::new(&self.repo)
    }

    /// Expand `~` and resolve a user-supplied path.
    pub fn resolve(&self, raw: &str) -> PathBuf {
        resolve_lenient(&expand_tilde(raw, &self.home))
    }

    /// `path` shown relative to the repo when inside it.
    pub fn repo_rel<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.repo).unwrap_or(path)
    }
}

fn home_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .context("$HOME is not set")?;
    Ok(resolve_lenient(Path::new(&home)))
}

/// `$DTFLS_REPO` if set, otherwise [`DEFAULT_REPO_DIR`]. Creates the directory
/// and initialises it as a git repo on first use.
pub fn resolve_repo(home: &Path) -> Result<PathBuf> {
    let raw = std::env::var("DTFLS_REPO")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_REPO_DIR.to_string());
    let repo = resolve_lenient(&expand_tilde(&raw, home));

    if !repo.exists() {
        std::fs::create_dir_all(&repo).with_context(|| format!("creating {}", repo.display()))?;
        ui::ok(&format!("Created dotfiles repo: {}", repo.display()));
    }
    if !repo.join(".git").is_dir() {
        if !git::init(&repo) {
            anyhow::bail!("git init failed in {}", repo.display());
        }
        ui::ok(&format!("Initialized git repo: {}", repo.display()));
    }
    Ok(repo)
}
