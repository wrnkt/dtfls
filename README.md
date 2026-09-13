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

The *optional* `.dtfls.json` configuration specifies what files to track, or ignore, as well as configurable mappings for where your
dotfiles are stored based on the system (`darwin`, `ubuntu`, `fedora`). Once you define these there's no need to
handle syncing files or directories manually.

Some commands to get started:
```bash
dtfls add [file-path]     # copies a file from the provided system path into the config repo (then commits)
dtfls sync                # l
dtfls info                # show config, mappings, and list status
dtfls sync                # syncs the tracked dotfiles in your config repo with your system files
dtfls status              # shows sync status / drif
dtfls diff                # diff all :deployed" system files vs. repo
dtfls diff ~/.bashrc      # diffs the indicated file
dtfls adopt               # copies all tracked system files into the repo
dtfls adopt --dry-run     # same as the above except it shows a preview of what would be copied
```
