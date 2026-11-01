# dtfls

A small CLI for keeping your dotfiles (or any config files) in a git repo and
syncing them between the repo and your machine. It ships as a single binary;
the only runtime dependency is `git`.

Works on macOS and Linux, with per-OS and per-distro overrides.

## Install

**From a release (prebuilt binary):** download the archive for your platform
from the [releases page](https://github.com/wrnkt/dtfls/releases), unpack it,
and put `dtfls` on your `PATH`.

**From source** (needs a [Rust toolchain](https://rustup.rs)):

```bash
cargo install --git https://github.com/wrnkt/dtfls --locked
```

Homebrew support is planned.

## Getting started

1. Run any `dtfls` command, e.g. `dtfls info`. On first use it creates the
   dotfiles repo at `~/.dtfls`, runs `git init` in it, and writes an empty
   `config.json`.
2. Add a file: `dtfls add ~/.bashrc`. It's copied into the repo, added to the
   `track` list, and committed.
3. Back the repo up: `cd ~/.dtfls`, add a git remote, and push once with
   `git push -u origin HEAD`. After that, `dtfls push` and `dtfls pull` work
   from anywhere.
4. On another machine, clone that remote to `~/.dtfls` and run
   `dtfls sync` to preview, then `dtfls sync --apply` to copy the files into
   place.

Run `dtfls --help` for the full reference, including the config schema.

## Commands

```bash
dtfls add <path>...       # copy files (or every file in a directory) into the repo, track them, commit
dtfls remove <path>...    # delete from the repo and commit; your system copy is left alone (alias: rm)
dtfls info                # repo, platform, active mappings, ignore patterns, tracked-file status
dtfls status              # per-file state: in sync / differs / not deployed
dtfls diff [path]         # unified diff of repo vs. deployed files
dtfls sync                # preview copying repo → system
dtfls sync --apply        # copy repo → system
dtfls adopt               # preview copying every tracked system file → repo
dtfls adopt --commit      # copy tracked system files → repo and commit
dtfls branches            # list host/ and backup/ branches
dtfls pull | push         # git pull / push the dotfiles repo
```

`sync` and `adopt` only preview unless you pass `--apply` / `--commit`.

Useful flags:

| Command | Flag | Effect |
|---|---|---|
| `sync` | `--no-pull` | Skip the `git pull --ff-only` that runs first |
| `sync` | `--host-branch` | Sync from `host/<hostname>` if it exists (else `main`/`master`), and snapshot any deployed file about to be overwritten into a `backup/<hostname>-<timestamp>` branch |
| `sync` | `--no-save` | With `--host-branch`: skip the snapshot |
| `adopt` | `--no-commit` | With `--commit`: write files into the repo but don't commit |
| `add`, `remove`, `adopt` | `-m <msg>` | Use your own commit message |

## Repo layout

With no `mappings` in the config, directories at the repo root map to `$HOME`
by convention, from general to specific. When the same file exists in more
than one, the most specific one wins.

```
home/           →  $HOME   (all platforms)
home.darwin/    →  $HOME   (macOS only)
home.linux/     →  $HOME   (Linux only)
home.ubuntu/    →  $HOME   (Ubuntu only — any distro ID from /etc/os-release works)
home.fedora/    →  $HOME   (Fedora only)
```

If none of these exist, the repo root itself is treated as `$HOME`.

## Configuration

`config.json` at the root of the repo. Every key is optional.

```json
{
  "track": [
    "~/.bashrc",
    "~/.config/nvim/init.lua",
    { "path": "~/.zprofile",     "os": "darwin" },
    { "path": "~/.bash_profile", "os": "linux", "distro": "fedora" },
    { "path": "~/.bashrc",       "src": "home.linux" }
  ],
  "ignore": ["*.swp", "secrets/"],
  "mappings": [
    { "src": "home",        "dest": "~" },
    { "src": "home.darwin", "dest": "~", "os": "darwin" },
    { "src": "home.fedora", "dest": "~", "os": "linux", "distro": "fedora" }
  ],
  "hooks": {
    "pre_sync":  ["brew bundle --file=Brewfile"],
    "post_sync": ["source ~/.zshrc 2>/dev/null || true"]
  }
}
```

- **`track`**: system files that `adopt` copies into the repo. `add` appends
  to this list for you. `src` pins an entry to a specific repo directory.
- **`ignore`**: glob patterns, matched against each path component. A trailing
  `/` names a directory. Added to the defaults (`.DS_Store`, `*.swp`, `*.swo`,
  `*.orig`, `.git`, `config.json`, `README*`, `LICENSE*`, `.gitignore`).
- **`mappings`**: replaces the `home*/` convention with explicit
  source → destination pairs, optionally filtered by `os` (`darwin`, `linux`)
  and `distro`.
- **`hooks`**: shell commands run from the repo root before and after `sync`.

## Branches

| Branch | Purpose |
|---|---|
| `main` / `master` | Default source |
| `host/<hostname>` | Machine-specific overrides, used by `sync --host-branch` |
| `backup/<hostname>-<timestamp>` | Created by `sync --host-branch` with the deployed state of files it was about to overwrite |

## Environment

| Variable | Effect |
|---|---|
| `DTFLS_REPO` | Use this repo location instead of `~/.dtfls` |
| `NO_COLOR` | Disable coloured output (also off when stdout isn't a terminal) |

## Development

```bash
cargo test                                  # unit + end-to-end CLI tests
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Pushing a `v*` tag that matches the version in `Cargo.toml` builds binaries for
macOS (arm64, x86_64) and Linux (x86_64, arm64) and attaches them to a GitHub
Release.

## License

MIT
