//! Read-only commands: `status`, `diff`, `branches`, `info`.

use anyhow::Result;
use similar::TextDiff;

use crate::config;
use crate::ctx::Ctx;
use crate::fsutil::{is_file, read_file};
use crate::routing::{active_dirs, branch_files, resolve_tracked_files, Ignore};
use crate::ui;

fn host_branch_line(ctx: &Ctx) {
    let c = ui::c();
    let host_branch = format!("host/{}", ctx.hostname);
    let tag = if ctx.git().branch_exists(&host_branch) {
        format!("{}exists{}", c.green, c.reset)
    } else {
        format!("{}not yet created{}", c.dim, c.reset)
    };
    ui::info(&format!("Host branch:  {host_branch}  ({tag})"));
}

pub fn status(ctx: &Ctx) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let dirs = active_dirs(ctx, &config);
    let ignore = Ignore::new(&config.ignore);
    let git = ctx.git();
    let branch = git.current_branch();
    let c = ui::c();

    ui::head("dtfls status");
    ui::info(&format!("Repo:         {}", ctx.repo.display()));
    ui::info(&format!("Branch:       {branch}"));
    ui::info(&format!("Platform:     {}", ctx.platform));
    host_branch_line(ctx);

    let (mut ok, mut differs, mut missing) = (0, 0, 0);
    for dir in &dirs {
        ui::head(&format!("{}  →  {}", dir.label(), dir.dest.display()));
        for f in branch_files(&git, &branch, &dir.name, &ignore) {
            let Some(content) = git.show_bytes(&branch, &f.repo_rel) else {
                continue;
            };
            let target = dir.dest.join(&f.rel);
            let rel = f.rel.display();
            if !target.exists() {
                println!(
                    "  {}✗{}  {rel}  {}(not deployed){}",
                    c.red, c.reset, c.dim, c.reset
                );
                missing += 1;
            } else if read_file(&target).as_deref() == Some(&content[..]) {
                ui::dim(&format!("✓  {rel}"));
                ok += 1;
            } else {
                println!(
                    "  {}≠{}  {rel}  {}(deployed copy differs){}",
                    c.yellow, c.reset, c.dim, c.reset
                );
                differs += 1;
            }
        }
    }

    println!();
    println!(
        "  {}{ok} in sync{} · {}{differs} differ{} · {}{missing} not deployed{}",
        c.green, c.reset, c.yellow, c.reset, c.red, c.reset
    );
    if differs > 0 || missing > 0 {
        ui::info("Run  dtfls sync  to update.");
    }
    Ok(())
}

pub fn diff(ctx: &Ctx, file: Option<&str>) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let dirs = active_dirs(ctx, &config);
    let ignore = Ignore::new(&config.ignore);
    let git = ctx.git();
    let branch = git.current_branch();
    let filter = file.map(|f| ctx.resolve(f));

    let mut found = false;
    for dir in &dirs {
        for f in branch_files(&git, &branch, &dir.name, &ignore) {
            let target = dir.dest.join(&f.rel);
            if filter.as_ref().is_some_and(|want| *want != target) {
                continue;
            }
            let Some(deployed) = read_file(&target) else {
                continue;
            };
            let Some(content) = git.show_bytes(&branch, &f.repo_rel) else {
                continue;
            };
            if content == deployed {
                continue;
            }

            let rel = f.rel.display();
            ui::head(&format!("diff: {rel}"));
            let (old_label, new_label) = (format!("repo/{rel}"), format!("deployed/{rel}"));
            match (
                std::str::from_utf8(&content),
                std::str::from_utf8(&deployed),
            ) {
                (Ok(old), Ok(new)) => print!(
                    "{}",
                    TextDiff::from_lines(old, new)
                        .unified_diff()
                        .header(&old_label, &new_label)
                ),
                _ => println!("Binary files {old_label} and {new_label} differ"),
            }
            found = true;
        }
    }

    if !found {
        ui::ok("No differences found.");
    }
    Ok(())
}

pub fn branches(ctx: &Ctx) {
    let git = ctx.git();
    let c = ui::c();
    let mine_host = format!("host/{}", ctx.hostname);

    ui::head(&format!(
        "Host branches  {}host/<hostname>{}",
        c.dim, c.reset
    ));
    let mut host = git.list_branches("host/");
    host.sort();
    if host.is_empty() {
        ui::dim("none — create one with: git checkout -b host/<hostname>");
    }
    for b in &host {
        let mine = if *b == mine_host {
            format!("  {}← this machine{}", c.green, c.reset)
        } else {
            String::new()
        };
        ui::info(&format!("{b}{mine}"));
    }

    ui::head(&format!(
        "Backup branches  {}backup/<hostname>-<timestamp>{}",
        c.dim, c.reset
    ));
    let mut backup = git.list_branches("backup/");
    backup.sort_by(|a, b| b.cmp(a));
    if backup.is_empty() {
        ui::dim("none");
    }
    let mine_prefix = format!("backup/{}-", ctx.hostname);
    for b in &backup {
        let mine = if b.starts_with(&mine_prefix) {
            format!("  {}(this machine){}", c.dim, c.reset)
        } else {
            String::new()
        };
        ui::info(&format!("{b}{mine}"));
    }
}

pub fn info(ctx: &Ctx) -> Result<()> {
    let config = config::load(&ctx.repo)?;
    let dirs = active_dirs(ctx, &config);
    let c = ui::c();

    ui::head("dtfls info");
    ui::info(&format!("Repo:         {}", ctx.repo.display()));
    ui::info(&format!("Branch:       {}", ctx.git().current_branch()));
    ui::info(&format!("Platform:     {}", ctx.platform));
    ui::info(&format!("Hostname:     {}", ctx.hostname));
    host_branch_line(ctx);

    ui::head("Active source mappings");
    for d in &dirs {
        ui::info(&format!("  {}  →  {}", d.label(), d.dest.display()));
    }

    ui::head("Ignore patterns");
    for pat in &config.ignore {
        ui::dim(pat);
    }

    if !config.track.is_empty() {
        ui::head(&format!(
            "Tracked files  {}({} entries){}",
            c.dim,
            config.track.len(),
            c.reset
        ));
        for t in resolve_tracked_files(ctx, &config) {
            let status = if is_file(&t.repo_path) {
                format!("{}✔ in repo{}", c.green, c.reset)
            } else {
                format!("{}⚠ not yet adopted{}", c.yellow, c.reset)
            };
            let disk = if is_file(&t.deployed) {
                String::new()
            } else {
                format!("  {}(not on disk){}", c.red, c.reset)
            };
            ui::info(&format!(
                "  {}  →  {}  {status}{disk}",
                t.deployed.display(),
                t.label()
            ));
        }
    }
    Ok(())
}
