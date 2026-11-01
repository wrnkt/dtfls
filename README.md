# dtfls

A Python CLI for managing files on your machine (especially config/dotfiles), backed by a git repo.

### Getting Started
1. Install: `pip install dtfls` (a `pipx install dtfls` is recommended to keep it isolated).
1. Run any `dtfls` command. On first use it creates and `git init`s the dotfiles repo at `~/.dtfls`.
1. `cd ~/.dtfls` and configure a git remote to back up your dotfiles.
1. Run `dtfls --help` for a simple overview.

### Notes
* The dotfiles repo defaults to `~/.dtfls`. Override the location with the `DTFLS_REPO` environment variable.

### Usage

The `config.json` at the root of the dotfiles repo (created empty on first run, and updated by `dtfls add`) specifies what files to track, or ignore, as well as configurable mappings for where your
dotfiles are stored based on the system (`darwin`, `ubuntu`, `fedora`). Once you define these there's no need to
handle syncing files or directories manually.

Some commands to get started:
```bash
dtfls add <path>          # copies a file (or every file in a directory) into the repo, tracks it, and commits
dtfls remove <path>       # deletes a file from the repo and commits; the copy on your system is left alone
dtfls info                # show config, mappings, and tracked-file status
dtfls status              # shows sync status / drift between repo and system
dtfls diff                # diff all deployed system files vs. repo
dtfls diff ~/.bashrc      # diffs the indicated file
dtfls sync                # preview what syncing repo → system would change
dtfls sync --apply        # copy the dotfiles in the repo onto your system
dtfls adopt               # preview copying all tracked system files into the repo
dtfls adopt --commit      # copy all tracked system files into the repo and commit
```
