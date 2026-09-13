# dtfls

A standalone Python script for managing files on your machine (especially config/dotfiles).

### Getting Started
1. Create an empty directory to keep backups of your dotfiles.
1. Initialize `git` and configure a Git remote for this repo.
1. Copy this repo's `./dtfls` script into your new directory.
1. If you'd like to invoke `dtfls` from anywhere, make it discoverable by adding this folder to your `PATH`
1. Ensure proper permissions are set on `./dtfls`. It should be readable and executable.
1. Run `dtfls --help` for a simple overview.

### Notes
* `dtfls` still relies on the script and saved files to be in the same directory
* the copied config file backups should go into `~/.local/share/dtfls/`
* the script or executable belongs in `~/.local/bin/dtfls` and if not so, through a symlink.
- [ ] Add configuration through an env variable to pick any location to store the data 

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
