//! `dtfls adopt`: copy every tracked file from the system into the repo.

use std::fs;

use anyhow::Result;

use crate::cmd::{stage_and_commit, summarize_paths};
use crate::config::{self, CONFIG_NAME};
use crate::ctx::Ctx;
use crate::fsutil::{is_file, read_file};
use crate::routing::resolve_tracked_files;
use crate::ui;

pub struct AdoptArgs {
    pub commit: bool,
    pub no_commit: bool,
    pub message: Option<String>,
}

/// Outcomes per file:
///   added   — copied into the repo for the first time
///   updated — repo copy differed from the system copy; repo updated
///   ok      — repo copy already identical
///   missing — not found on this machine
pub fn run(ctx: &Ctx, args: &AdoptArgs) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let dry = !args.commit;
    let c = ui::c();

    if dry {
        ui::head("Preview — pass --commit to adopt changes");
    }

    let tracked = resolve_tracked_files(ctx, &config);
    if tracked.is_empty() {
        ui::warn(&format!(
            "No files listed in the 'track' key of {CONFIG_NAME}."
        ));
        ui::warn(r#"Add entries like:  "track": ["~/.bashrc", "~/.config/nvim/init.lua"]"#);
        return Ok(());
    }

    ui::head(&format!("Adopting {} tracked file(s)", tracked.len()));

    let (mut added, mut updated, mut ok, mut missing) = (0, 0, 0, 0);
    let mut changed = Vec::new();

    for t in &tracked {
        let label = t.label();
        let Some(incoming) = read_file(&t.deployed) else {
            ui::warn(&format!(
                "missing   {label}  {}({} not found){}",
                c.dim,
                t.deployed.display(),
                c.reset
            ));
            missing += 1;
            continue;
        };

        let tag = if is_file(&t.repo_path) {
            if read_file(&t.repo_path).as_deref() == Some(&incoming[..]) {
                ui::dim(&format!("✓  {label}"));
                ok += 1;
                continue;
            }
            "updated"
        } else {
            "added"
        };

        if !dry {
            if let Some(parent) = t.repo_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&t.repo_path, &incoming)?;
            changed.push(t.repo_path.clone());
        }

        ui::ok(&format!(
            "{tag:9}  {label}  {}← {}{}",
            c.dim,
            t.deployed.display(),
            c.reset
        ));
        if tag == "added" {
            added += 1;
        } else {
            updated += 1;
        }
    }

    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("{added} added"));
    }
    if updated > 0 {
        parts.push(format!("{updated} updated"));
    }
    if ok > 0 {
        parts.push(format!("{ok} already up to date"));
    }
    if missing > 0 {
        parts.push(format!("{}{missing} missing{}", c.yellow, c.reset));
    }
    println!();
    ui::ok(&format!("Done.  {}", parts.join(" · ")));

    if dry {
        ui::warn("(preview — nothing was changed)");
        return Ok(());
    }
    if changed.is_empty() {
        return Ok(());
    }

    if args.no_commit {
        ui::info("Files written to repo — commit skipped (--no-commit).");
        ui::info("Stage and commit manually when ready.");
        return Ok(());
    }

    let msg = args
        .message
        .clone()
        .unwrap_or_else(|| format!("adopt: {}", summarize_paths(ctx, &changed, "…")));
    let r = stage_and_commit(ctx, &changed, &msg);
    if r.ok {
        ui::ok(&format!("Committed {} file(s): {msg}", changed.len()));
    } else {
        ui::warn("git commit failed — files are staged but not committed.");
        ui::warn(r.stderr.trim());
    }
    Ok(())
}
