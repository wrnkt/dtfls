//! dtfls — portable dotfile sync from a git repo.
//!
//! Manages a git repo of dotfiles. The repo location is `$DTFLS_REPO` if set,
//! otherwise `~/.dtfls` (created, and initialised as a git repo, on first use).

mod cmd;
mod config;
mod ctx;
mod fsutil;
mod git;
mod interrupt;
mod platform;
mod routing;
mod ui;

use clap::{CommandFactory, Parser, Subcommand};

use crate::cmd::add::AddArgs;
use crate::cmd::adopt::AdoptArgs;
use crate::cmd::remove::RemoveArgs;
use crate::cmd::sync::SyncArgs;
use crate::ctx::Ctx;

const EPILOG: &str = r#"repo layout (convention, no config file required):
  home/                 →  $HOME           (all platforms)
  home.darwin/          →  $HOME           (macOS only)
  home.linux/           →  $HOME           (Linux only)
  home.ubuntu/          →  $HOME           (Ubuntu only)
  home.fedora/          →  $HOME           (Fedora only)

branch conventions:
  main / master             default source branch
  host/<hostname>           machine-specific config — use with --host-branch
  backup/<hostname>-<ts>    auto-snapshot of deployed state before overwrite

config.json schema (all keys optional):
  {
    "track": [
      "~/.bashrc",
      "~/.config/nvim/init.lua",
      { "path": "~/.zprofile",      "os": "darwin" },
      { "path": "~/.bash_profile",  "os": "linux", "distro": "fedora" },
      { "path": "~/.bashrc",        "src": "home.linux" }
    ],
    "ignore": ["*.swp", "secrets/"],
    "mappings": [
      { "src": "home",         "dest": "~" },
      { "src": "home.darwin",  "dest": "~", "os": "darwin" },
      { "src": "home.fedora",  "dest": "~", "os": "linux", "distro": "fedora" }
    ],
    "hooks": {
      "pre_sync":  ["brew bundle --file=Brewfile"],
      "post_sync": ["source ~/.zshrc 2>/dev/null || true"]
    }
  }

examples:
  dtfls adopt                             preview what would be adopted (default)
  dtfls adopt --commit                    copy tracked system files into repo and commit
  dtfls adopt --commit --no-commit        write files to repo but skip the git commit
  dtfls adopt --commit -m "snapshot"
  dtfls sync                              preview what would be synced (default)
  dtfls sync --apply                      copy files from repo to system
  dtfls sync --apply --host-branch        use host/<hostname> branch; auto-snapshot before overwrite
  dtfls sync --apply --host-branch --no-save  use host branch, skip snapshot
  dtfls sync --apply --no-pull            skip git pull
  dtfls add ~/.bashrc                     track a single new file and commit
  dtfls add ~/.config/nvim/init.lua -m "track neovim config"
  dtfls status                            per-file sync state
  dtfls diff                              diff all deployed files vs repo
  dtfls diff ~/.bashrc                    diff one file
  dtfls branches                          list host/ and backup/ branches
  dtfls info                              show config, mappings, track list status"#;

#[derive(Parser)]
#[command(
    name = "dtfls",
    version,
    about = "Portable dotfile sync. Manages your dotfiles repo (default: ~/.dtfls, override with $DTFLS_REPO).",
    after_help = EPILOG
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Copy all files listed in 'track' from system into repo
    Adopt {
        /// Actually write files to repo and commit (default is preview only)
        #[arg(long)]
        commit: bool,
        /// With --commit: write files to repo but skip the git commit
        #[arg(long)]
        no_commit: bool,
        /// Override the auto-generated git commit message
        #[arg(short, long, value_name = "msg")]
        message: Option<String>,
    },
    /// Copy dotfiles from repo to system
    Sync {
        /// Actually copy files to system (default is preview only)
        #[arg(long)]
        apply: bool,
        /// Skip git pull before syncing
        #[arg(long)]
        no_pull: bool,
        /// Select source branch automatically: uses host/<hostname> if it
        /// exists, otherwise falls back to the default branch (main/master).
        /// Also creates a backup/<hostname>-<timestamp> branch capturing the
        /// current deployed state of any file that would be overwritten.
        #[arg(long)]
        host_branch: bool,
        /// With --host-branch: skip the backup snapshot branch
        #[arg(long)]
        no_save: bool,
    },
    /// Track one or more files: copy into repo and commit
    Add {
        #[arg(value_name = "file", required = true)]
        files: Vec<String>,
        /// Git commit message
        #[arg(short, long, value_name = "msg")]
        message: Option<String>,
    },
    /// Stop tracking files/dirs: delete from repo, leave deployed copy
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(value_name = "file", required = true)]
        files: Vec<String>,
        /// Git commit message
        #[arg(short, long, value_name = "msg")]
        message: Option<String>,
    },
    /// Show per-file sync state
    Status,
    /// Diff deployed files vs repo versions
    Diff {
        /// Limit to one file
        #[arg(value_name = "file")]
        file: Option<String>,
    },
    /// List all host/ and backup/ branches
    Branches,
    /// Show config, active mappings, host-branch status
    Info,
    /// git pull the dotfile repo
    Pull,
    /// git push the dotfile repo
    Push,
}

fn main() {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        let _ = Cli::command().print_help();
        return;
    };

    interrupt::install();
    if let Err(e) = run(command) {
        ui::err(&format!("{e:#}"));
        std::process::exit(1);
    }
}

fn run(command: Command) -> anyhow::Result<()> {
    let ctx = Ctx::from_env()?;
    config::ensure(&ctx.repo)?;

    match command {
        Command::Adopt {
            commit,
            no_commit,
            message,
        } => cmd::adopt::run(
            &ctx,
            &AdoptArgs {
                commit,
                no_commit,
                message,
            },
        ),
        Command::Sync {
            apply,
            no_pull,
            host_branch,
            no_save,
        } => cmd::sync::run(
            &ctx,
            &SyncArgs {
                apply,
                no_pull,
                host_branch,
                no_save,
            },
        ),
        Command::Add { files, message } => cmd::add::run(&ctx, &AddArgs { files, message }),
        Command::Remove { files, message } => {
            cmd::remove::run(&ctx, &RemoveArgs { files, message })
        }
        Command::Status => cmd::inspect::status(&ctx),
        Command::Diff { file } => cmd::inspect::diff(&ctx, file.as_deref()),
        Command::Branches => {
            cmd::inspect::branches(&ctx);
            Ok(())
        }
        Command::Info => cmd::inspect::info(&ctx),
        Command::Pull => {
            cmd::pull(&ctx);
            Ok(())
        }
        Command::Push => {
            cmd::push(&ctx);
            Ok(())
        }
    }
}
