# Status

| Item | State |
|---|---|
| Branch review | Done — see `branch-report.md` |
| PR #4 (PyPI + Homebrew, Python) | Held as reference; retargeted to `main` |
| Delete 3 merged branches | **Pending** — blocked from the agent session (HTTP 403); needs a manual delete |
| Phase 1: fix Python first | Done — PR #8 (merged), see `main-fixes.md` |
| Phase 2: crate + base modules | Done — PR #9 (merged) |
| Phase 3: all commands | Done — `info`, `status`, `diff`, `branches`, `pull`, `push`, `add`, `remove`/`rm`, `adopt`, `sync` (incl. `--host-branch` backup) |
| Phase 4: parity | Done — `scripts/parity.sh` gave identical output to Python across the scenario (script removed at cutover) |
| Phase 5: CI | Done — fmt / clippy / test on Ubuntu + macOS |
| Phase 6a: cutover | Done — `claude/rust-cutover`: Python package, tests, parity script and Python CI removed; README rewritten; `release.yml` builds Rust binaries |
| Phase 6b: Homebrew | **Not started** — see "Next" |

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

## Next (phase 6b: Homebrew)

`release.yml` already attaches macOS / Linux tarballs (+ `.sha256`) to each
GitHub Release, which is what a formula needs.

1. Create the public repo `wrnkt/homebrew-dtfls`.
2. Add a `HOMEBREW_TAP_TOKEN` secret to this repo (fine-grained PAT,
   `contents:write` on the tap).
3. Either run `dist init` with the homebrew installer (replaces
   `release.yml`), or add a job that writes `Formula/dtfls.rb` pointing at
   the release tarballs — see PR #4's `homebrew` job for the bump step.
4. Tag v0.2.0. Close PR #4 once its tap/token notes are carried over.
