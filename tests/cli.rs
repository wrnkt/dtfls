//! End-to-end tests: run the built binary against a temp $HOME and repo.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;

struct Env {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let home = base.join("home");
        fs::create_dir(&home).unwrap();
        Env {
            _tmp: tmp,
            repo: base.join("repo"),
            home,
        }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("dtfls").unwrap();
        c.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &self.home)
            .env("DTFLS_REPO", &self.repo)
            .env("NO_COLOR", "1")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .current_dir(&self.home);
        c
    }

    fn git(&self, args: &[&str]) -> String {
        let out = StdCommand::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", &self.home)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    fn write(&self, path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Repo with a committed home/ dir holding `.bashrc`.
    fn with_committed_bashrc(&self, text: &str) {
        self.cmd().arg("info").assert().success();
        self.write(&self.repo.join("home/.bashrc"), text);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", "init"]);
    }
}

#[test]
fn no_command_prints_help_and_exits_zero() {
    let env = Env::new();
    env.cmd()
        .assert()
        .success()
        .stdout(contains("Usage: dtfls"))
        .stdout(contains("config.json schema"));
    assert!(!env.repo.exists(), "help must not create the repo");
}

#[test]
fn first_run_creates_repo_and_config() {
    let env = Env::new();
    env.cmd()
        .arg("info")
        .assert()
        .success()
        .stdout(contains("Created dotfiles repo"))
        .stdout(contains("Initialized git repo"))
        .stdout(contains("(repo root)"));
    assert!(env.repo.join(".git").is_dir());
    assert_eq!(
        fs::read_to_string(env.repo.join("config.json")).unwrap(),
        "{}\n"
    );
}

#[test]
fn default_repo_location_is_dot_dtfls() {
    let env = Env::new();
    env.cmd()
        .env_remove("DTFLS_REPO")
        .arg("info")
        .assert()
        .success();
    assert!(env.home.join(".dtfls/.git").is_dir());
}

#[test]
fn existing_repo_is_not_reinitialised() {
    let env = Env::new();
    env.cmd().arg("info").assert().success();
    fs::write(env.repo.join("marker.txt"), "keep me").unwrap();
    env.cmd()
        .arg("info")
        .assert()
        .success()
        .stdout(contains("Initialized").not());
    assert!(env.repo.join("marker.txt").exists());
}

#[test]
fn add_copies_tracks_and_commits() {
    let env = Env::new();
    env.cmd().arg("info").assert().success();
    fs::create_dir(env.repo.join("home")).unwrap();
    env.write(&env.home.join(".bashrc"), "echo hi\n");

    env.cmd()
        .args(["add", "~/.bashrc"])
        .assert()
        .success()
        .stdout(contains("Copied to repo: home/.bashrc"))
        .stdout(contains("Committed 1 file(s): add home/.bashrc"));

    assert_eq!(
        fs::read_to_string(env.repo.join("home/.bashrc")).unwrap(),
        "echo hi\n"
    );
    let cfg = fs::read_to_string(env.repo.join("config.json")).unwrap();
    assert!(cfg.contains("\"~/.bashrc\""));
    assert_eq!(env.git(&["status", "--porcelain"]), "");
    assert!(env
        .git(&["log", "--format=%s"])
        .contains("add home/.bashrc"));
}

#[test]
fn add_directory_without_tty_adds_every_file() {
    let env = Env::new();
    env.cmd().arg("info").assert().success();
    env.write(&env.home.join(".config/nvim/init.lua"), "x");
    env.write(&env.home.join(".config/nvim/lua/a.lua"), "y");

    env.cmd()
        .args(["add", "~/.config/nvim"])
        .assert()
        .success()
        .stdout(contains("2 file(s) will be tracked"))
        .stdout(contains("Committed 2 file(s)"));
    assert!(env.repo.join(".config/nvim/lua/a.lua").is_file());
}

#[test]
fn add_reports_missing_path() {
    let env = Env::new();
    env.cmd()
        .args(["add", "~/nope"])
        .assert()
        .success()
        .stderr(contains("Not found"));
}

#[test]
fn status_and_diff_report_drift() {
    let env = Env::new();
    env.with_committed_bashrc("one\n");

    env.cmd()
        .arg("status")
        .assert()
        .success()
        .stdout(contains(".bashrc  (not deployed)"))
        .stdout(contains("0 in sync · 0 differ · 1 not deployed"));

    env.write(&env.home.join(".bashrc"), "two\n");
    env.cmd()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("(deployed copy differs)"));
    env.cmd()
        .args(["diff", "~/.bashrc"])
        .assert()
        .success()
        .stdout(contains("--- repo/.bashrc"))
        .stdout(contains("+++ deployed/.bashrc"))
        .stdout(contains("-one"))
        .stdout(contains("+two"));
}

#[test]
fn sync_previews_by_default_and_applies_with_flag() {
    let env = Env::new();
    env.with_committed_bashrc("repo version\n");

    env.cmd()
        .args(["sync", "--no-pull"])
        .assert()
        .success()
        .stdout(contains("[preview]  .bashrc"))
        .stdout(contains("(preview — nothing was changed)"));
    assert!(!env.home.join(".bashrc").exists());

    env.cmd()
        .args(["sync", "--no-pull", "--apply"])
        .assert()
        .success()
        .stdout(contains("copied   .bashrc"))
        .stdout(contains("Done.  1 copied"));
    assert_eq!(
        fs::read_to_string(env.home.join(".bashrc")).unwrap(),
        "repo version\n"
    );

    env.cmd()
        .args(["sync", "--no-pull", "--apply"])
        .assert()
        .success()
        .stdout(contains("1 already up to date"));
}

#[test]
fn sync_skips_ignored_files_and_runs_hooks() {
    let env = Env::new();
    env.cmd().arg("info").assert().success();
    env.write(
        &env.repo.join("config.json"),
        r#"{"ignore": ["secrets/"], "hooks": {"post_sync": ["touch hooked"]}}"#,
    );
    env.write(&env.repo.join("home/.bashrc"), "x");
    env.write(&env.repo.join("home/secrets/token"), "s");
    env.write(&env.repo.join("home/.vimrc.swp"), "s");
    env.git(&["add", "-A"]);
    env.git(&["commit", "-qm", "init"]);

    env.cmd()
        .args(["sync", "--no-pull", "--apply"])
        .assert()
        .success()
        .stdout(contains("$ touch hooked"));
    assert!(env.home.join(".bashrc").exists());
    assert!(!env.home.join("secrets").exists());
    assert!(!env.home.join(".vimrc.swp").exists());
    assert!(env.repo.join("hooked").exists());
}

#[test]
fn sync_host_branch_snapshots_deployed_state_and_restores_branch() {
    let env = Env::new();
    env.with_committed_bashrc("repo version\n");
    env.write(&env.home.join(".bashrc"), "local edits\n");
    let before = env.git(&["rev-parse", "--abbrev-ref", "HEAD"]);

    env.cmd()
        .args(["sync", "--no-pull", "--apply", "--host-branch"])
        .assert()
        .success()
        .stdout(contains("1 deployed file(s) will change"))
        .stdout(contains("Snapshot saved → backup/"));

    assert_eq!(env.git(&["rev-parse", "--abbrev-ref", "HEAD"]), before);
    assert_eq!(
        fs::read_to_string(env.home.join(".bashrc")).unwrap(),
        "repo version\n"
    );
    let backups = env.git(&["branch", "--list", "backup/*", "--format=%(refname:short)"]);
    let backup = backups.trim();
    assert!(!backup.is_empty());
    assert_eq!(
        env.git(&["show", &format!("{backup}:home/.bashrc")]),
        "local edits\n"
    );

    env.cmd()
        .arg("branches")
        .assert()
        .success()
        .stdout(contains(backup).and(contains("(this machine)")));
}

#[test]
fn adopt_previews_then_commits() {
    let env = Env::new();
    env.cmd().arg("info").assert().success();
    fs::create_dir(env.repo.join("home")).unwrap();
    env.write(
        &env.repo.join("config.json"),
        r#"{"track": ["~/.bashrc", "~/.missing"]}"#,
    );
    env.git(&["add", "-A"]);
    env.git(&["commit", "-qm", "init"]);
    env.write(&env.home.join(".bashrc"), "mine\n");

    env.cmd()
        .arg("adopt")
        .assert()
        .success()
        .stdout(contains("added      home/.bashrc"))
        .stdout(contains("missing   home/.missing"))
        .stdout(contains("(preview — nothing was changed)"));
    assert!(!env.repo.join("home/.bashrc").exists());

    env.cmd()
        .args(["adopt", "--commit", "-m", "snapshot"])
        .assert()
        .success()
        .stdout(contains("Committed 1 file(s): snapshot"));
    assert_eq!(
        fs::read_to_string(env.repo.join("home/.bashrc")).unwrap(),
        "mine\n"
    );
    assert!(env
        .git(&["log", "-1", "--format=%s"])
        .starts_with("snapshot"));
}

#[test]
fn remove_deletes_from_repo_but_keeps_deployed_copy() {
    let env = Env::new();
    env.with_committed_bashrc("x\n");
    env.write(&env.home.join(".bashrc"), "x\n");

    env.cmd()
        .args(["rm", "~/.bashrc"])
        .assert()
        .success()
        .stdout(contains("Removed from repo: home/.bashrc"))
        .stdout(contains("Committed removal of 1 file(s)"));
    assert!(!env.repo.join("home/.bashrc").exists());
    assert!(env.home.join(".bashrc").exists());
    assert_eq!(env.git(&["status", "--porcelain"]), "");
}

#[test]
fn invalid_config_is_a_clean_error() {
    let env = Env::new();
    env.cmd().arg("info").assert().success();
    fs::write(env.repo.join("config.json"), "{ nope").unwrap();
    env.cmd()
        .arg("status")
        .assert()
        .failure()
        .code(1)
        .stderr(contains("parsing"));
}
