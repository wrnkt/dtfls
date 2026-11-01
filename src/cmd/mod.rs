//! Subcommand implementations.

pub mod add;
pub mod adopt;
pub mod inspect;
pub mod remove;
pub mod sync;

use std::path::PathBuf;

use crate::ctx::Ctx;
use crate::ui;

/// Default commit message body: the first three paths, then a count.
pub(crate) fn summarize_paths(ctx: &Ctx, paths: &[PathBuf], ellipsis: &str) -> String {
    let mut names = paths
        .iter()
        .take(3)
        .map(|p| ctx.repo_rel(p).display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > 3 {
        names.push_str(&format!(", {ellipsis} (+{} more)", paths.len() - 3));
    }
    names
}

/// Stage *paths* and commit; returns whether the commit succeeded.
pub(crate) fn stage_and_commit(ctx: &Ctx, paths: &[PathBuf], msg: &str) -> crate::git::GitOutput {
    let git = ctx.git();
    for p in paths {
        git.run(&["add", "--", &p.to_string_lossy()]);
    }
    git.run(&["commit", "-m", msg])
}

pub fn pull(ctx: &Ctx) {
    ui::head(&format!("git pull  [{}]", repo_name(ctx)));
    ctx.git().run_inherit(&["pull"]);
}

pub fn push(ctx: &Ctx) {
    ui::head(&format!("git push  [{}]", repo_name(ctx)));
    ctx.git().run_inherit(&["push"]);
}

fn repo_name(ctx: &Ctx) -> String {
    ctx.repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
