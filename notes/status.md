# Status

| Item | State |
|---|---|
| Branch review | Done — see `branch-report.md` |
| PR #4 (PyPI + Homebrew, Python) | Held as reference; retargeted to `main` |
| Delete 3 merged branches | **Pending** — blocked from the agent session (HTTP 403); needs a manual delete |
| Phase 1: fix Python first | Done — PR #8 (`claude/fix-main-issues`), see `main-fixes.md` |
| Phase 2: crate + base modules | Done — `claude/rust-port` |
| Phase 3: all commands | Done — `info`, `status`, `diff`, `branches`, `pull`, `push`, `add`, `remove`/`rm`, `adopt`, `sync` (incl. `--host-branch` backup) |
| Phase 4: parity | Done — `scripts/parity.sh` gives identical output to Python across the scenario; 23 unit + 14 CLI tests |
| Phase 5: CI | Done — fmt / clippy / test on Ubuntu + macOS, Python tests, parity job |
| Phase 6: cutover + Homebrew | **Not started** — see "Next" |

## Rust layout

```
src/main.rs        clap CLI + dispatch
src/ctx.rs         Ctx { repo, home, platform, hostname }; repo resolution
src/platform.rs    OS / distro / hostname
src/config.rs      config.json load/merge/save, track list
src/routing.rs     active_dirs, route, resolve_tracked_files, ignore, branch_files
src/fsutil.rs      ~ expansion, resolve_lenient, apply_copy, walk_files
src/git.rs         git CLI wrapper
src/ui.rs          output styles, NO_COLOR / tty detection
src/interrupt.rs   Ctrl-C: exit 130, deferred during the backup branch switch
src/cmd/           add, adopt, remove, sync, inspect (status/diff/branches/info), pull/push
tests/cli.rs       end-to-end tests against temp $HOME + repo
scripts/parity.sh  Python vs Rust output comparison (delete at cutover)
```

## Deliberate differences from the Python version

- **Help output** is clap's format, and gains `--version` and `help <cmd>`.
- **Destination dirs and `$HOME` are symlink-resolved**, like deployed paths
  already were. Python compared resolved file paths against unresolved
  `$HOME`, which breaks routing when home (or macOS's `/var` → `/private/var`)
  is a symlink.
- **Errors are one line, exit 1**, not tracebacks: invalid `config.json`, an
  explicit-`src` track entry outside its `dest`, a failed write during `sync`
  (reported per file; the rest continue).
- **`diff` is built in** (`similar`), so no `diff` binary is needed; binary
  files print "Binary files … differ".
- **Non-ASCII filenames** work: `git ls-tree -z` instead of git's quoted output.
- **Repo-root labels**: `add` prints `.bashrc`, not `./.bashrc`.
- **`branches`** marks backups as "this machine" by the `backup/<host>-`
  prefix rather than a substring match.
- **Ctrl-C during the backup branch switch** is deferred until the original
  branch is restored, then exits 130.
- **`add` copies keep permissions but not mtime** (Python's `copy2` kept both).
- **Not ported:** `adopt --interactive`, which was behind a `False` feature
  flag in Python and never reachable.

## Next (phase 6)

1. Merge PR #8, then the port PR.
2. Cutover PR: delete `src/dtfls/`, `tests/*.py`, `pyproject.toml`,
   `scripts/parity.sh`, the Python and parity CI jobs; rewrite the README
   install section for Homebrew / `cargo install`.
3. Homebrew via `dist`: create `wrnkt/homebrew-dtfls`, add the
   `HOMEBREW_TAP_TOKEN` secret, `dist init` (homebrew installer), tag v0.2.0.
   Close PR #4 once its tap/token notes are carried over.
