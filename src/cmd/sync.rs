//! `dtfls sync`: copy dotfiles from the repo onto the system.

use std::path::PathBuf;

use anyhow::Result;

use crate::config::{self, CONFIG_NAME};
use crate::ctx::Ctx;
use crate::fsutil::{apply_copy, copy_file, read_file, CopyStatus};
use crate::interrupt;
use crate::routing::{active_dirs, branch_files, Ignore, SrcDir};
use crate::ui;

pub struct SyncArgs {
    pub apply: bool,
    pub no_pull: bool,
    pub host_branch: bool,
    pub no_save: bool,
}

/// One file to write: content read from the source branch up front.
struct Planned {
    repo_rel: String,
    content: Vec<u8>,
    target: PathBuf,
    label: String,
}

pub fn run(ctx: &Ctx, args: &SyncArgs) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let git = ctx.git();
    let dry = !args.apply;
    let c = ui::c();

    if dry {
        ui::head("Preview — pass --apply to sync files to system");
    }

    // ── Source branch ────────────────────────────────────────────────────
    let source_branch = if args.host_branch {
        let host_branch = format!("host/{}", ctx.hostname);
        if git.branch_exists(&host_branch) {
            ui::info(&format!(
                "Host branch:   {}{host_branch}{}",
                c.cyan, c.reset
            ));
            host_branch
        } else {
            let b = git.default_branch();
            ui::info(&format!(
                "No host branch for '{}' — using: {}{b}{}",
                ctx.hostname, c.cyan, c.reset
            ));
            ui::info(&format!("  Tip: git checkout -b host/{}", ctx.hostname));
            b
        }
    } else {
        let b = git.current_branch();
        ui::info(&format!("Source branch: {}{b}{}", c.cyan, c.reset));
        b
    };
    ui::info(&format!("Platform:      {}", ctx.platform));

    // ── Pull ─────────────────────────────────────────────────────────────
    if !args.no_pull {
        ui::head("Pulling latest");
        let ok = git
            .run_inherit(&["pull", "--ff-only"])
            .is_some_and(|s| s.success());
        if !ok {
            ui::warn("git pull failed — continuing with local state");
        }
    }

    // ── Pre-sync hooks ───────────────────────────────────────────────────
    if !config.hooks.pre_sync.is_empty() {
        ui::head("Pre-sync hooks");
        run_hooks(ctx, &config.hooks.pre_sync);
    }

    // ── Plan ─────────────────────────────────────────────────────────────
    // Read all content up front via git show, before the backup branch
    // temporarily switches the working tree.
    let dirs = active_dirs(ctx, &config);
    if dirs.is_empty() {
        ui::warn("No source directories found.");
        ui::warn(&format!(
            "Create a home/ dir in your repo, or add mappings to {CONFIG_NAME}."
        ));
        return Ok(());
    }
    let ignore = Ignore::new(&config.ignore);
    let mut plan: Vec<(SrcDir, Vec<Planned>)> = Vec::new();
    for dir in &dirs {
        let mut entries = Vec::new();
        for f in branch_files(&git, &source_branch, &dir.name, &ignore) {
            match git.show_bytes(&source_branch, &f.repo_rel) {
                Some(content) => entries.push(Planned {
                    target: dir.dest.join(&f.rel),
                    label: f.rel.display().to_string(),
                    repo_rel: f.repo_rel,
                    content,
                }),
                None => ui::warn(&format!(
                    "  Could not read {} from '{source_branch}'",
                    f.repo_rel
                )),
            }
        }
        plan.push((dir.clone(), entries));
    }

    // ── Backup branch (before any writes) ────────────────────────────────
    if args.host_branch && !args.no_save && !dry {
        let to_backup: Vec<(String, PathBuf)> = plan
            .iter()
            .flat_map(|(_, entries)| entries)
            .filter(|p| read_file(&p.target).is_some_and(|cur| cur != p.content))
            .map(|p| (p.repo_rel.clone(), p.target.clone()))
            .collect();
        if to_backup.is_empty() {
            ui::dim("Deployed files match repo — no backup branch needed");
        } else {
            ui::head(&format!(
                "{} deployed file(s) will change — saving snapshot first",
                to_backup.len()
            ));
            create_backup_branch(ctx, &to_backup);
        }
    }

    // ── Apply ────────────────────────────────────────────────────────────
    let (mut ok, mut copied) = (0usize, 0usize);
    for (dir, entries) in &plan {
        ui::head(&format!("{}  →  {}", dir.label(), dir.dest.display()));
        for p in entries {
            match apply_copy(&p.content, &p.target, dry) {
                Ok(CopyStatus::Ok) => {
                    ok += 1;
                    ui::dim(&format!("✓  {}", p.label));
                }
                Ok(CopyStatus::Copied) => {
                    copied += 1;
                    let tag = if dry { "[preview]" } else { "copied" };
                    ui::ok(&format!("{tag:7}  {}", p.label));
                }
                Err(e) => ui::err(&format!("{}: {e}", p.target.display())),
            }
        }
    }

    // ── Post-sync hooks ──────────────────────────────────────────────────
    if !config.hooks.post_sync.is_empty() {
        ui::head("Post-sync hooks");
        run_hooks(ctx, &config.hooks.post_sync);
    }

    // ── Summary ──────────────────────────────────────────────────────────
    let mut parts = Vec::new();
    if copied > 0 {
        parts.push(format!("{copied} copied"));
    }
    if ok > 0 {
        parts.push(format!("{ok} already up to date"));
    }
    println!();
    ui::ok(&format!("Done.  {}", parts.join(" · ")));
    if dry {
        ui::warn("(preview — nothing was changed)");
    }
    Ok(())
}

fn run_hooks(ctx: &Ctx, hooks: &[String]) {
    for hook in hooks {
        ui::info(&format!("$ {hook}"));
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(hook)
            .current_dir(&ctx.repo)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => ui::warn(&format!(
                "Hook exited {}",
                s.code().map_or("by signal".into(), |c| c.to_string())
            )),
            Err(e) => ui::warn(&format!("Hook failed to start: {e}")),
        }
    }
}

/// Switches back to the original branch when dropped, so the repo is never
/// left on the backup branch — even on error or Ctrl-C.
struct RestoreBranch<'a> {
    ctx: &'a Ctx,
    original: String,
}

impl Drop for RestoreBranch<'_> {
    fn drop(&mut self) {
        let r = self.ctx.git().run(&["checkout", &self.original]);
        if !r.ok {
            ui::err(&format!(
                "FAILED to restore branch '{0}' — run: git checkout {0}",
                self.original
            ));
        }
    }
}

/// Snapshot the current deployed state of files about to change into a new
/// `backup/<hostname>-<timestamp>` branch, then return to the original branch.
fn create_backup_branch(ctx: &Ctx, to_backup: &[(String, PathBuf)]) -> Option<String> {
    let git = ctx.git();
    let c = ui::c();
    if !git.is_clean() {
        ui::warn("Working tree has uncommitted changes — skipping backup branch.");
        ui::warn("Commit or stash repo changes first, then re-run.");
        return None;
    }

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let branch = format!("backup/{}-{ts}", ctx.hostname);
    let original = git.current_branch();
    ui::info(&format!(
        "Creating backup branch: {}{branch}{}",
        c.cyan, c.reset
    ));

    let _critical = interrupt::Critical::enter();
    let r = git.run(&["checkout", "-b", &branch]);
    if !r.ok {
        ui::warn(&format!("Could not create branch: {}", r.stderr.trim()));
        return None;
    }
    let _restore = RestoreBranch { ctx, original };

    for (repo_rel, deployed) in to_backup {
        if let Err(e) = copy_file(deployed, &ctx.repo.join(repo_rel)) {
            ui::err(&format!("Backup branch failed: {e}"));
            return None;
        }
    }
    git.run(&["add", "-A"]);
    let msg = format!("backup: deployed state on {} at {ts}", ctx.hostname);
    let r = git.run(&["commit", "-m", &msg]);
    if r.ok {
        ui::ok(&format!("Snapshot saved → {branch}"));
        Some(branch)
    } else {
        if format!("{}{}", r.stdout, r.stderr).contains("nothing to commit") {
            ui::dim("Deployed files match repo HEAD — no backup commit needed");
        } else {
            ui::warn(&format!("git commit failed: {}", r.stderr.trim()));
        }
        None
    }
}
