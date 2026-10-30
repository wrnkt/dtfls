//! Thin wrapper around the `git` CLI, always run against the dotfiles repo.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};

pub struct Git {
    repo: PathBuf,
}

/// Captured result of a git command.
pub struct GitOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Git {
    pub fn new(repo: &Path) -> Self {
        Git {
            repo: repo.to_path_buf(),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&self.repo).args(args);
        cmd
    }

    fn output(&self, args: &[&str]) -> Option<Output> {
        self.command(args).stdin(Stdio::null()).output().ok()
    }

    /// Run with stdout/stderr captured.
    pub fn run(&self, args: &[&str]) -> GitOutput {
        match self.output(args) {
            Some(o) => GitOutput {
                ok: o.status.success(),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            },
            None => GitOutput {
                ok: false,
                stdout: String::new(),
                stderr: "could not run git — is it installed?".into(),
            },
        }
    }

    /// Run with output going straight to the terminal.
    pub fn run_inherit(&self, args: &[&str]) -> Option<ExitStatus> {
        self.command(args).status().ok()
    }

    pub fn branch_exists(&self, branch: &str) -> bool {
        self.run(&["rev-parse", "--verify", "--quiet", branch]).ok
    }

    pub fn current_branch(&self) -> String {
        let r = self.run(&["rev-parse", "--abbrev-ref", "HEAD"]);
        if r.ok {
            r.stdout.trim().to_string()
        } else {
            "HEAD".into()
        }
    }

    /// `main`, `master`, or whatever the current branch is.
    pub fn default_branch(&self) -> String {
        for b in ["main", "master"] {
            if self.branch_exists(b) {
                return b.into();
            }
        }
        self.current_branch()
    }

    pub fn is_clean(&self) -> bool {
        let r = self.run(&["status", "--porcelain"]);
        r.ok && r.stdout.trim().is_empty()
    }

    /// Repo-relative file paths in *branch* under *prefix*.
    pub fn ls_files(&self, branch: &str, prefix: &str) -> Vec<String> {
        let r = self.run(&["ls-tree", "-r", "--name-only", "-z", branch, "--", prefix]);
        if !r.ok {
            return Vec::new();
        }
        r.stdout
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Raw bytes of *branch*:*repo_rel* — never touches the working tree.
    pub fn show_bytes(&self, branch: &str, repo_rel: &str) -> Option<Vec<u8>> {
        let spec = format!("{branch}:{repo_rel}");
        let o = self.output(&["show", &spec])?;
        o.status.success().then_some(o.stdout)
    }

    pub fn list_branches(&self, prefix: &str) -> Vec<String> {
        let pattern = format!("{prefix}*");
        let r = self.run(&["branch", "--list", &pattern, "--format=%(refname:short)"]);
        if !r.ok {
            return Vec::new();
        }
        r.stdout
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }
}

/// `git init -q <dir>`.
pub fn init(dir: &Path) -> bool {
    Command::new("git")
        .args(["init", "-q"])
        .arg(dir)
        .stdin(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
