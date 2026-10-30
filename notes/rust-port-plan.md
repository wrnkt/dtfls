# Plan: port dtfls to Rust and ship it through Homebrew

The tool is one ~1,360-line Python file with no third-party dependencies that
shells out to `git`. That makes it a good fit for a single static binary.

## Crate layout

The global `REPO` becomes an explicit context struct, so tests stop relying on
monkeypatching.

```
Cargo.toml
src/main.rs       clap (derive) definitions and dispatch; Ctrl-C exits 130
src/ctx.rs        struct Ctx { repo, home, platform } — injectable for tests
src/platform.rs   detect_os(), /etc/os-release, short hostname
src/config.rs     serde: Config, TrackEntry (String | {path, os, distro, src, dest}), Mapping, Hooks
src/routing.rs    active_dirs, resolve_tracked_files, one route() shared by add/remove/adopt
src/git.rs        thin wrapper that runs `git -C <repo>`
src/fsutil.rs     ~ expansion, lenient path resolution, apply_copy, directory walk
src/ui.rs         ✔ → ⚠ ✖ output; colour only on a tty and when NO_COLOR is unset
src/cmd/          sync, adopt, add, remove, status, diff, branches, info, pull/push
```

## Library choices

| Area | Choice | Why |
|---|---|---|
| Git | Keep running the `git` CLI | Same behaviour. Pull/push keep using the user's SSH and credential helpers. No libgit2/OpenSSL in the Homebrew build |
| CLI parsing | `clap` | Keeps the long help text, including the layout and schema epilog |
| Config | `serde`, `serde_json` (`preserve_order`) | Rewriting `config.json` won't reorder keys |
| Ignore patterns | `globset` | Per-path-component matching, as `fnmatch` is used now |
| Walk / temp / time | `walkdir`, `tempfile`, `chrono` | Replace the standard-library modules in use |
| Diff | `similar` | Unified diffs in-process; no runtime dependency on `diff` |
| Hostname | `gethostname` | Same short-hostname behaviour |

**Trap:** Python's `Path.resolve()` works on paths that don't exist yet; Rust's
`canonicalize()` fails on them. A `resolve_lenient()` must canonicalize the
deepest existing ancestor and append the rest. `track` entries and
`diff <file>` depend on this.

## Phases

1. **Fix the Python behaviour first** so the port copies corrected behaviour.
   See `main-fixes.md`.
2. **Base modules.** Scaffold the crate next to `src/dtfls/` (they coexist).
   Port `platform`, `config`, `routing`, `fsutil`; turn the pytest cases into
   Rust unit tests built on `Ctx`.
3. **Commands, read-only first:** `info` → `status` → `diff` → `branches` →
   `pull`/`push` → `add` → `remove` → `adopt` → `sync`. `sync` goes last for
   the backup branch (its always-restore step becomes a drop guard) and hooks.
4. **Parity.** `assert_cmd` integration tests with `HOME` / `DTFLS_REPO`
   pointed at temp dirs. While both exist, run Python and Rust on the same
   fixture repo with `NO_COLOR=1` and diff their output.
5. **CI.** `cargo fmt --check`, `clippy -D warnings`, `cargo test` on Ubuntu
   and macOS. Drop the Python matrix at cutover.
6. **Cut over.** Delete `src/dtfls/`, `tests/*.py`, `pyproject.toml`; fix the
   README; release v0.2.0. No PyPI.

## Homebrew

**Recommended:** `dist` (formerly cargo-dist) publishing to a personal tap.
It generates the release workflow (prebuilt macOS arm64/x86_64 and Linux
binaries, a GitHub Release, and a formula pushed to the tap). Users run
`brew install wrnkt/dtfls/dtfls` and compile nothing.

1. `dist init`, choose the `homebrew` installer, `tap = "wrnkt/homebrew-dtfls"`.
   This replaces `release.yml`.
2. Create the public repo `wrnkt/homebrew-dtfls`.
3. Add the `HOMEBREW_TAP_TOKEN` secret (fine-grained PAT, `contents:write` on
   the tap) — same step as PR #4's `RELEASING.md`.
4. Tag `v0.2.0` and push the tag.

**Simpler alternative:** a source-build formula, bumped by PR #4's existing
action. Fewer moving parts, but every user compiles on install.

```ruby
class Dtfls < Formula
  desc "Portable dotfile sync from a git repo"
  homepage "https://github.com/wrnkt/dtfls"
  url "https://github.com/wrnkt/dtfls/archive/refs/tags/v0.2.0.tar.gz"
  sha256 "…"
  license "MIT"
  depends_on "rust" => :build
  def install = system("cargo", "install", *std_cargo_args)
  test do
    ENV["DTFLS_REPO"] = testpath/"repo"
    system bin/"dtfls", "info"
    assert_predicate testpath/"repo/.git", :directory?
  end
end
```

`homebrew-core` (plain `brew install dtfls`) requires meeting Homebrew's
notability bar, so treat it as a later step.
