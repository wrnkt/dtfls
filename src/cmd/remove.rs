//! `dtfls remove`: stop tracking files/dirs — delete them from the repo and
//! commit. The deployed copy on the system is left untouched.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cmd::summarize_paths;
use crate::config;
use crate::ctx::Ctx;
use crate::fsutil::{relative_to, walk_files};
use crate::routing::{active_dirs, display_in_src, SrcDir};
use crate::ui;

pub struct RemoveArgs {
    pub files: Vec<String>,
    pub message: Option<String>,
}

pub fn run(ctx: &Ctx, args: &RemoveArgs) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let dirs = active_dirs(ctx, &config);
    let mut removed: Vec<PathBuf> = Vec::new();

    for raw in &args.files {
        let mut path = ctx.resolve(raw);

        // Accept either the deployed path or the repo path.
        if path.starts_with(&ctx.repo) {
            if let Some(deployed) = dirs
                .iter()
                .find_map(|d| relative_to(&path, &d.root(&ctx.repo)).map(|rel| d.dest.join(rel)))
            {
                path = deployed;
            }
        }
        remove_one(ctx, &dirs, &path, &mut removed)?;
    }

    if removed.is_empty() {
        return Ok(());
    }

    let git = ctx.git();
    let mut rm_args: Vec<String> = vec![
        "rm".into(),
        "--cached".into(),
        "--ignore-unmatch".into(),
        "-q".into(),
        "--".into(),
    ];
    rm_args.extend(removed.iter().map(|p| p.to_string_lossy().into_owned()));
    git.run(&rm_args.iter().map(String::as_str).collect::<Vec<_>>());

    let n = removed.len();
    let msg = args
        .message
        .clone()
        .unwrap_or_else(|| format!("remove {}", summarize_paths(ctx, &removed, "...")));
    if git.run(&["commit", "-m", &msg]).ok {
        ui::ok(&format!("Committed removal of {n} file(s): {msg}"));
    } else {
        ui::warn("git commit failed — deletions staged but not committed.");
    }
    Ok(())
}

/// `(src_dir, rel, repo_path)` for a deployed path. Prefers the most specific
/// dir that actually holds it in the repo, else the most specific that covers it.
fn find_repo_path<'a>(
    ctx: &Ctx,
    dirs: &'a [SrcDir],
    deployed: &Path,
) -> Option<(&'a SrcDir, PathBuf, PathBuf)> {
    let candidates: Vec<_> = dirs
        .iter()
        .filter_map(|d| {
            let rel = relative_to(deployed, &d.dest)?;
            let repo_path = d.root(&ctx.repo).join(&rel);
            Some((d, rel, repo_path))
        })
        .collect();
    let existing = candidates.iter().rposition(|(_, _, p)| p.exists());
    let idx = existing.or(candidates.len().checked_sub(1))?;
    candidates.into_iter().nth(idx)
}

fn remove_one(
    ctx: &Ctx,
    dirs: &[SrcDir],
    deployed: &Path,
    removed: &mut Vec<PathBuf>,
) -> Result<()> {
    let Some((dir, rel, repo_path)) = find_repo_path(ctx, dirs, deployed) else {
        ui::warn(&format!("Cannot map to repo: {}", deployed.display()));
        return Ok(());
    };
    let label = display_in_src(&dir.name, &rel);

    if !repo_path.exists() {
        ui::warn(&format!("Not tracked in repo: {label}"));
        return Ok(());
    }

    if repo_path.is_dir() {
        let files = walk_files(&repo_path);
        if files.is_empty() {
            ui::warn(&format!("Tracked dir is empty: {label}"));
            return Ok(());
        }
        for f in &files {
            fs::remove_file(f)?;
        }
        removed.extend(files.iter().cloned());
        remove_empty_dirs(&repo_path);
        ui::ok(&format!(
            "Removed from repo: {label}/  ({} file(s))",
            files.len()
        ));
    } else {
        fs::remove_file(&repo_path)?;
        removed.push(repo_path);
        ui::ok(&format!("Removed from repo: {label}"));
    }
    Ok(())
}

/// Remove *dir* and any now-empty subdirectories; non-empty ones are kept.
fn remove_empty_dirs(dir: &Path) {
    for entry in walkdir::WalkDir::new(dir)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
    {
        let _ = fs::remove_dir(entry.path());
    }
}
