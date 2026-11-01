//! `dtfls add`: copy files or directories into the repo, track them, commit.

use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cmd::{stage_and_commit, summarize_paths};
use crate::config::{self, CONFIG_NAME};
use crate::ctx::Ctx;
use crate::fsutil::{copy_file, relative_to, walk_files};
use crate::routing::{active_dirs, display_in_src, route, SrcDir};
use crate::ui;

pub struct AddArgs {
    pub files: Vec<String>,
    pub message: Option<String>,
}

pub fn run(ctx: &Ctx, args: &AddArgs) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let dirs = active_dirs(ctx, &config);

    let mut written: Vec<PathBuf> = Vec::new(); // repo paths, for git add
    let mut newly_tracked: Vec<PathBuf> = Vec::new(); // system paths, for config

    for raw in &args.files {
        let path = ctx.resolve(raw);
        if !path.exists() {
            ui::err(&format!("Not found: {}", path.display()));
            continue;
        }

        if path.is_dir() {
            let files = walk_files(&path);
            if files.is_empty() {
                ui::warn(&format!(
                    "Empty directory, nothing to add: {}",
                    path.display()
                ));
                continue;
            }
            ui::info(&format!(
                "Directory: {}  ({} file(s) will be tracked)",
                path.display(),
                files.len()
            ));
            for f in &files {
                ui::dim(&f.strip_prefix(&path).unwrap_or(f).display().to_string());
            }
            if std::io::stdin().is_terminal()
                && !ui::confirm(&format!("\n  Add these {} file(s)? [y/N] ", files.len()))
            {
                ui::info("Skipped.");
                continue;
            }
            for f in &files {
                add_one(ctx, &dirs, f, &mut written, &mut newly_tracked)?;
            }
        } else {
            add_one(ctx, &dirs, &path, &mut written, &mut newly_tracked)?;
        }
    }

    let track_strings: Vec<String> = newly_tracked
        .iter()
        .map(|p| match relative_to(p, &ctx.home) {
            Some(rel) => format!("~/{}", rel.display()),
            None => p.display().to_string(),
        })
        .collect();
    config::add_to_track(&ctx.repo, &track_strings)?;

    if written.is_empty() {
        return Ok(());
    }

    // Commit the updated track list alongside the files.
    let mut to_stage = written.clone();
    let cfg_file = ctx.repo.join(CONFIG_NAME);
    if cfg_file.exists() {
        to_stage.push(cfg_file);
    }

    let msg = args
        .message
        .clone()
        .unwrap_or_else(|| format!("add {}", summarize_paths(ctx, &written, "...")));
    let r = stage_and_commit(ctx, &to_stage, &msg);
    if r.ok {
        ui::ok(&format!("Committed {} file(s): {msg}", written.len()));
    } else {
        ui::warn("git commit failed — files are staged but not committed.");
    }
    Ok(())
}

/// Route and copy a single file into the repo tree.
fn add_one(
    ctx: &Ctx,
    dirs: &[SrcDir],
    file: &Path,
    written: &mut Vec<PathBuf>,
    newly_tracked: &mut Vec<PathBuf>,
) -> Result<()> {
    let fallback = SrcDir::new("home", ctx.home.clone());
    let dir = route(file, dirs).unwrap_or(&fallback);
    let Some(rel) = relative_to(file, &dir.dest) else {
        ui::err(&format!(
            "Cannot add {}: not under any mapped destination",
            file.display()
        ));
        return Ok(());
    };
    let label = display_in_src(&dir.name, &rel);
    let repo_file = dir.root(&ctx.repo).join(&rel);

    if repo_file.exists() {
        let incoming = fs::read(file)?;
        if fs::read(&repo_file)? == incoming {
            ui::dim(&format!("✓  already up to date: {label}"));
        } else {
            fs::write(&repo_file, &incoming)?;
            ui::ok(&format!("Updated in repo: {label}"));
            written.push(repo_file);
        }
    } else {
        copy_file(file, &repo_file)?;
        ui::ok(&format!("Copied to repo: {label}"));
        written.push(repo_file);
    }
    newly_tracked.push(file.to_path_buf());
    Ok(())
}
