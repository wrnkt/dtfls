"""
dtfls — portable dotfile sync from a git repo.

Manages a git repo of dotfiles. The repo location is resolved at startup:
the DTFLS_REPO environment variable if set, otherwise DEFAULT_REPO_DIR
(created, and initialized as a git repo, on first use).

Python 3.8+  |  no external dependencies  |  macOS · Fedora · Ubuntu

Repo layout convention (no config required):
  home/            → $HOME           (all platforms)
  home.darwin/     → $HOME           (macOS only)
  home.linux/      → $HOME           (Linux only)
  home.ubuntu/     → $HOME           (Ubuntu only)
  home.fedora/     → $HOME           (Fedora only)

  config.json    optional config at repo root (see --help for schema)

Branch conventions:
  main / master    default source
  host/<host>      machine-specific overrides (used with --host-branch)
  backup/<host>-*  auto-created snapshots of deployed state
"""

import argparse
import fnmatch
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Tuple

# Resolved at CLI startup by resolve_repo() — see main().
REPO: Optional[Path] = None
CONFIG_NAME = "config.json"
DEFAULT_REPO_DIR = "~/.dtfls"  # used unless $DTFLS_REPO overrides it

FEATURE_INTERACTIVE = False  # enable to expose --interactive on `adopt`


# ── Terminal output ───────────────────────────────────────────────────────────


class C:
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    CYAN = "\033[36m"


if not sys.stdout.isatty() or os.environ.get("NO_COLOR"):
    for _attr in list(vars(C)):
        if not _attr.startswith("_"):
            setattr(C, _attr, "")


def _ok(msg: str):
    print(f"  {C.GREEN}✔{C.RESET}  {msg}")


def _info(msg: str):
    print(f"  {C.BLUE}→{C.RESET}  {msg}")


def _warn(msg: str):
    print(f"  {C.YELLOW}⚠{C.RESET}  {msg}")


def _err(msg: str):
    print(f"  {C.RED}✖{C.RESET}  {msg}", file=sys.stderr)


def _dim(msg: str):
    print(f"     {C.DIM}{msg}{C.RESET}")


def _head(msg: str):
    print(f"\n{C.BOLD}{C.CYAN}{msg}{C.RESET}")


# ── OS / host detection ───────────────────────────────────────────────────────


def detect_os() -> Tuple[str, Optional[str]]:
    """Return (platform, distro). E.g. ('darwin', None) or ('linux', 'ubuntu')."""
    p = sys.platform
    if p == "darwin":
        return "darwin", None
    if p.startswith("linux"):
        return "linux", _linux_distro()
    return p, None


def _linux_distro() -> Optional[str]:
    try:
        with open("/etc/os-release") as f:
            for line in f:
                if line.startswith("ID="):
                    return line.split("=", 1)[1].strip().strip('"').lower()
    except FileNotFoundError:
        pass
    return None


def get_hostname() -> str:
    """Short hostname, no domain suffix."""
    return socket.gethostname().split(".")[0]


# ── Repo resolution ───────────────────────────────────────────────────────────


def resolve_repo() -> Path:
    """
    Resolve the dotfiles repo location: $DTFLS_REPO if set, otherwise
    DEFAULT_REPO_DIR. Creates the directory and initializes it as a git
    repo on first use.
    """
    env = os.environ.get("DTFLS_REPO")
    repo = Path(env or DEFAULT_REPO_DIR).expanduser().resolve()

    if not repo.exists():
        repo.mkdir(parents=True)
        _ok(f"Created dotfiles repo: {repo}")

    if not (repo / ".git").is_dir():
        subprocess.run(["git", "init", "-q", str(repo)])
        _ok(f"Initialized git repo: {repo}")

    return repo


# ── Git helpers ───────────────────────────────────────────────────────────────


def _git(*args: str, capture: bool = True) -> subprocess.CompletedProcess:
    cmd = ["git", "-C", str(REPO)] + list(args)
    if capture:
        return subprocess.run(cmd, capture_output=True, text=True)
    return subprocess.run(cmd)


def git_branch_exists(branch: str) -> bool:
    return _git("rev-parse", "--verify", branch).returncode == 0


def git_current_branch() -> str:
    r = _git("rev-parse", "--abbrev-ref", "HEAD")
    return r.stdout.strip() if r.returncode == 0 else "HEAD"


def git_default_branch() -> str:
    """Return 'main', 'master', or whatever the current branch is."""
    for b in ("main", "master"):
        if git_branch_exists(b):
            return b
    return git_current_branch()


def git_is_clean() -> bool:
    r = _git("status", "--porcelain")
    return r.returncode == 0 and not r.stdout.strip()


def git_ls_files(branch: str, prefix: str) -> List[str]:
    """Repo-relative file paths in *branch* under *prefix*/."""
    r = _git("ls-tree", "-r", "--name-only", branch, "--", prefix)
    if r.returncode != 0:
        return []
    return [line.strip() for line in r.stdout.splitlines() if line.strip()]


def git_show_bytes(branch: str, repo_rel: str) -> Optional[bytes]:
    """Raw bytes of a file at *branch*:*repo_rel* — never touches working tree."""
    cmd = ["git", "-C", str(REPO), "show", f"{branch}:{repo_rel}"]
    r = subprocess.run(cmd, capture_output=True)
    return r.stdout if r.returncode == 0 else None


def git_list_branches(prefix: str) -> List[str]:
    r = _git("branch", "--list", f"{prefix}*", "--format=%(refname:short)")
    if r.returncode != 0:
        return []
    return [line.strip() for line in r.stdout.splitlines() if line.strip()]


# ── Path ────────────────────────────────────────────────────────────────────-


def is_git(p: Path):
    """Return if path includes a .git dir"""
    return ".git" in p.parts


# ── Config ────────────────────────────────────────────────────────────────────

DEFAULT_IGNORE: List[str] = [
    ".DS_Store",
    "*.swp",
    "*.swo",
    "*.orig",
    ".git",
    CONFIG_NAME,
    "README*",
    "LICENSE*",
    ".gitignore",
]


def load_user_config() -> Dict:
    """Load raw config from REPO/config.json. Returns {} if absent."""
    cfg_file = REPO / CONFIG_NAME
    if not cfg_file.exists():
        return {}
    with open(cfg_file) as f:
        return json.load(f)


def save_user_config(user_cfg: Dict) -> None:
    """Write user config to REPO/config.json."""
    cfg_file = REPO / CONFIG_NAME
    with open(cfg_file, "w") as f:
        json.dump(user_cfg, f, indent=2)
        f.write("\n")


def ensure_config() -> None:
    """Create REPO/config.json with empty defaults if it does not exist."""
    cfg_file = REPO / CONFIG_NAME
    if not cfg_file.exists():
        save_user_config({})


def load_config() -> Dict:
    cfg: Dict = {
        "ignore": list(DEFAULT_IGNORE),
        "hooks": {"pre_sync": [], "post_sync": []},
    }
    user = load_user_config()
    if not user:
        return cfg
    if "ignore" in user:
        seen = set(cfg["ignore"])
        for pat in user["ignore"]:
            if pat not in seen:
                cfg["ignore"].append(pat)
                seen.add(pat)
    for key in ("hooks", "mappings", "track"):
        if key in user:
            cfg[key] = user[key]
    return cfg


# ── Source → destination mappings ────────────────────────────────────────────


def active_dirs(config: Dict) -> List[Tuple[str, Path]]:
    """
    Returns (repo_dirname, dest_path) pairs active for this OS, in order of
    increasing specificity (home → home.linux → home.fedora).
    """
    plat, distro = detect_os()
    active: List[Tuple[str, Path]] = []

    if "mappings" in config:
        for m in config["mappings"]:
            if m.get("os") and m["os"] != plat:
                continue
            if m.get("distro") and m["distro"] != distro:
                continue
            if (REPO / m["src"]).is_dir():
                active.append((m["src"], Path(m["dest"]).expanduser()))
    else:
        candidates = ["home", f"home.{plat}"]
        if distro:
            candidates.append(f"home.{distro}")
        for name in candidates:
            if (REPO / name).is_dir():
                active.append((name, Path.home()))

        # No home/ subdirs found — treat repo root itself as $HOME.
        if not active:
            active.append((".", Path.home()))

    return active


# ── Tracked-file resolution ───────────────────────────────────────────────────


def resolve_tracked_files(
    config: Dict,
) -> List[Tuple[Path, str, Path]]:
    """
    Expand the ``track`` list from config into concrete file tuples.

    Each entry in ``track`` is either a plain path string or an object::

        "~/.bashrc"
        { "path": "~/.zprofile",   "os": "darwin" }
        { "path": "~/.bash_profile", "os": "linux", "distro": "fedora" }
        { "path": "~/.bashrc", "src": "home.linux" }   # explicit bucket

    Returns [(deployed_path, src_name, repo_dest_path)] for every entry that
    is active on this machine.  The file does not need to exist yet — callers
    decide how to handle missing files.

    Routing logic (no explicit ``src``):
      Use the *most specific* active src_dir whose dest_dir is a parent of the
      deployed path.  "Most specific" = last entry in active_dirs(), which is
      ordered general → specific (home → home.linux → home.ubuntu).
    """
    plat, distro = detect_os()
    dirs = active_dirs(config)  # [(src_name, dest_dir)] general→specific
    track = config.get("track", [])
    result: List[Tuple[Path, str, Path]] = []

    for entry in track:
        # Normalise to dict.
        if isinstance(entry, str):
            entry = {"path": entry}

        # Platform / distro filter.
        if entry.get("os") and entry["os"] != plat:
            continue
        if entry.get("distro") and entry["distro"] != distro:
            continue

        deployed = Path(entry["path"]).expanduser().resolve()

        # Explicit src bucket override.
        if "src" in entry:
            src_name = entry["src"]
            dest_dir = Path(entry.get("dest", "~")).expanduser()
            repo_dest = REPO / src_name / deployed.relative_to(dest_dir)
            result.append((deployed, src_name, repo_dest))
            continue

        # Auto-route: find the most specific active dir that covers this file.
        matched: Optional[Tuple[str, Path]] = None
        for src_name, dest_dir in dirs:  # general→specific order
            try:
                deployed.relative_to(dest_dir)  # raises if not under dest_dir
                matched = (src_name, dest_dir)
            except ValueError:
                continue

        if matched is None:
            _warn(f"Cannot route {entry['path']} to any active src dir — skipping")
            continue

        src_name, dest_dir = matched
        repo_dest = REPO / src_name / deployed.relative_to(dest_dir)
        result.append((deployed, src_name, repo_dest))

    return result


def _is_ignored(path: Path, patterns: List[str]) -> bool:
    return any(fnmatch.fnmatch(path.name, p) for p in patterns)


def iter_branch_files(
    branch: str,
    src_name: str,
    ignore: List[str],
) -> List[Tuple[str, Path]]:
    """
    Return [(repo_rel_path, rel_to_src)] for all tracked, non-ignored files in
    *branch* under src_name/. Uses git ls-tree — never needs a branch checkout.
    """
    results: List[Tuple[str, Path]] = []
    for repo_rel in git_ls_files(branch, src_name + "/"):
        p = Path(repo_rel)
        if any(_is_ignored(Path(part), ignore) for part in p.parts):
            continue
        results.append((repo_rel, p.relative_to(src_name)))
    return results


# ── Apply (copy only) ─────────────────────────────────────────────────────────


def apply_copy(content: bytes, target: Path, dry: bool) -> str:
    """
    Write *content* to *target* as a plain copy.
    Returns 'ok' (identical, no write needed) or 'copied'.
    Handles dangling symlinks and existing files gracefully.
    """
    # is_file() returns False for dangling symlinks, so check content only when it exists.
    if target.is_file() and target.read_bytes() == content:
        return "ok"
    if not dry:
        target.parent.mkdir(parents=True, exist_ok=True)
        # Remove dangling symlinks or existing files so write_bytes creates fresh.
        if target.is_symlink() or target.exists():
            target.unlink()
        target.write_bytes(content)
    return "copied"


# ── Backup branch ──────────────────────────────────────────────────────────────


def create_backup_branch(
    hostname: str,
    to_backup: List[Tuple[str, Path]],  # (repo_rel_path, deployed_path)
) -> Optional[str]:
    """
    Capture the *current deployed state* of files that are about to change,
    storing them in a new branch backup/<hostname>-<timestamp>.

    Approach — no external tools needed:
      1. Require a clean working tree (deployed files live outside the repo).
      2. git checkout -b backup/<hostname>-<ts>   (new branch from HEAD — no changes yet)
      3. Copy each deployed file into the repo tree at its tracked path.
      4. git add -A && git commit
      5. git checkout <original>                  (always runs, even on error)

    After step 5, the working tree is back to the original branch state.
    We read all source-branch content via git-show *before* this function is
    called, so the checkout-and-restore never interferes with syncing.
    """
    if not to_backup:
        return None

    if not git_is_clean():
        _warn("Working tree has uncommitted changes — skipping backup branch.")
        _warn("Commit or stash repo changes first, then re-run.")
        return None

    ts = datetime.now().strftime("%Y%m%d_%H%M%S")
    branch = f"backup/{hostname}-{ts}"
    original = git_current_branch()
    created = False

    _info(f"Creating backup branch: {C.CYAN}{branch}{C.RESET}")

    try:
        r = _git("checkout", "-b", branch)
        if r.returncode != 0:
            _warn(f"Could not create branch: {r.stderr.strip()}")
            return None

        for repo_rel, deployed_path in to_backup:
            dest = REPO / repo_rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(str(deployed_path), str(dest))

        _git("add", "-A")
        msg = f"backup: deployed state on {hostname} at {ts}"
        r = _git("commit", "-m", msg)

        if r.returncode == 0:
            created = True
            _ok(f"Snapshot saved → {branch}")
        else:
            # Nothing committed (no differences) or commit failed.
            if "nothing to commit" in r.stdout + r.stderr:
                _dim("Deployed files match repo HEAD — no backup commit needed")
            else:
                _warn(f"git commit failed: {r.stderr.strip()}")

    except Exception as exc:
        _err(f"Backup branch failed: {exc}")
    finally:
        # Critical: always restore, even if an exception occurred above.
        restore = _git("checkout", original)
        if restore.returncode != 0:
            _err(
                f"FAILED to restore branch '{original}' — run: git checkout {original}"
            )

    return branch if created else None


# ── Hooks ─────────────────────────────────────────────────────────────────────


def run_hooks(hooks: List[str]) -> None:
    for hook in hooks:
        _info(f"$ {hook}")
        r = subprocess.run(hook, shell=True, cwd=REPO)
        if r.returncode != 0:
            _warn(f"Hook exited {r.returncode}")


# ── sync ──────────────────────────────────────────────────────────────────────


def cmd_sync(args: argparse.Namespace) -> None:
    config = load_config()
    dry = not args.apply
    hostname = get_hostname()
    ignore = config.get("ignore", DEFAULT_IGNORE)
    plat, distro = detect_os()

    if dry:
        _head("Preview — pass --apply to sync files to system")

    # ── Determine source branch ──────────────────────────────────────────────
    if args.host_branch:
        host_branch = f"host/{hostname}"
        if git_branch_exists(host_branch):
            source_branch = host_branch
            _info(f"Host branch:   {C.CYAN}{source_branch}{C.RESET}")
        else:
            source_branch = git_default_branch()
            _info(
                f"No host branch for '{hostname}' — using: {C.CYAN}{source_branch}{C.RESET}"
            )
            _info(f"  Tip: git checkout -b host/{hostname}")
    else:
        source_branch = git_current_branch()
        _info(f"Source branch: {C.CYAN}{source_branch}{C.RESET}")

    _info(f"Platform:      {plat}" + (f" / {distro}" if distro else ""))

    # ── Pull ─────────────────────────────────────────────────────────────────
    if not args.no_pull:
        _head("Pulling latest")
        r = _git("pull", "--ff-only", capture=False)
        if r.returncode != 0:
            _warn("git pull failed — continuing with local state")

    # ── Pre-sync hooks ───────────────────────────────────────────────────────
    pre_hooks = config.get("hooks", {}).get("pre_sync", [])
    if pre_hooks:
        _head("Pre-sync hooks")
        run_hooks(pre_hooks)

    # ── Build sync plan ──────────────────────────────────────────────────────
    # Read ALL file content up front via git-show (no working-tree checkout).
    # This must happen before create_backup_branch, which temporarily switches
    # branches — after which git-show still works but our bytes are already in memory.
    dirs = active_dirs(config)
    if not dirs:
        _warn("No source directories found.")
        _warn(f"Create a home/ dir in your repo, or add mappings to {CONFIG_NAME}.")
        return

    # plan: src_name → [(content_bytes, target_path, display_label)]
    plan: Dict[str, List[Tuple[bytes, Path, str]]] = {}

    for src_name, dest_dir in dirs:
        entries: List[Tuple[bytes, Path, str]] = []
        for repo_rel, rel_to_src in iter_branch_files(source_branch, src_name, ignore):
            content = git_show_bytes(source_branch, repo_rel)
            if content is None:
                _warn(f"  Could not read {repo_rel} from '{source_branch}'")
                continue
            entries.append((content, dest_dir / rel_to_src, str(rel_to_src)))
        plan[src_name] = entries

    # ── Backup branch (before any writes) ────────────────────────────────────
    if args.host_branch and not args.no_save and not dry:
        # Identify deployed files that differ from what we're about to write.
        to_backup: List[Tuple[str, Path]] = []
        for src_name, dest_dir in dirs:
            for repo_rel, rel_to_src in iter_branch_files(
                source_branch, src_name, ignore
            ):
                target = dest_dir / rel_to_src
                if not target.is_file():
                    continue
                # Look up pre-read content for this target.
                for content, t, _ in plan.get(src_name, []):
                    if t == target and target.read_bytes() != content:
                        to_backup.append((repo_rel, target))
                        break

        if to_backup:
            _head(
                f"{len(to_backup)} deployed file(s) will change — saving snapshot first"
            )
            create_backup_branch(hostname, to_backup)
        else:
            _dim("Deployed files match repo — no backup branch needed")

    # ── Apply ────────────────────────────────────────────────────────────────
    counts: Dict[str, int] = {"ok": 0, "copied": 0}

    for src_name, dest_dir in dirs:
        src_label = "(repo root)" if src_name == "." else f"{src_name}/"
        _head(f"{src_label}  →  {dest_dir}")
        for content, target, label in plan.get(src_name, []):
            status = apply_copy(content, target, dry)
            counts[status] = counts.get(status, 0) + 1
            if status == "ok":
                _dim(f"✓  {label}")
            else:
                tag = "[preview]" if dry else "copied"
                _ok(f"{tag:7}  {label}")

    # ── Post-sync hooks ──────────────────────────────────────────────────────
    post_hooks = config.get("hooks", {}).get("post_sync", [])
    if post_hooks:
        _head("Post-sync hooks")
        run_hooks(post_hooks)

    # ── Summary ──────────────────────────────────────────────────────────────
    parts = []
    if counts.get("copied"):
        parts.append(f"{counts['copied']} copied")
    if counts.get("ok"):
        parts.append(f"{counts['ok']} already up to date")
    print()
    _ok("Done.  " + " · ".join(parts))
    if dry:
        _warn("(preview — nothing was changed)")


# ── adopt ─────────────────────────────────────────────────────────────────────


def cmd_adopt(args: argparse.Namespace) -> None:
    """
    Bulk-copy every file in the ``track`` list from the system into the repo,
    then optionally commit.

    Outcomes per file:
      added    — file copied into repo for the first time
      updated  — file already in repo but deployed copy differed; repo updated
      ok       — repo copy already identical; skipped
      missing  — file not found on this machine; skipped with warning
    """
    config = load_config()
    dry = not args.commit

    if dry:
        _head("Preview — pass --commit to adopt changes")

    tracked = resolve_tracked_files(config)
    if not tracked:
        _warn(f"No files listed in the 'track' key of {CONFIG_NAME}.")
        _warn('Add entries like:  "track": ["~/.bashrc", "~/.config/nvim/init.lua"]')
        sys.exit(0)

    _head(f"Adopting {len(tracked)} tracked file(s)")

    counts: Dict[str, int] = {"added": 0, "updated": 0, "ok": 0, "missing": 0}
    changed_paths: List[Path] = []  # repo paths that were written, for git add

    for deployed, src_name, repo_dest in tracked:
        label = (
            str(repo_dest.relative_to(REPO))
            if src_name == "."
            else f"{src_name}/{repo_dest.relative_to(REPO / src_name)}"
        )

        if not deployed.is_file():
            _warn(f"missing   {label}  {C.DIM}({deployed} not found){C.RESET}")
            counts["missing"] += 1
            continue

        incoming = deployed.read_bytes()

        if repo_dest.is_file():
            if repo_dest.read_bytes() == incoming:
                _dim(f"✓  {label}")
                counts["ok"] += 1
                continue
            tag = "updated"
        else:
            tag = "added"

        if getattr(args, "interactive", False) and tag == "updated" and not dry:
            _head(f"diff: {label}  {C.DIM}(repo → system){C.RESET}")
            suffix = deployed.suffix or ""
            with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as tmp:
                tmp.write(repo_dest.read_bytes())
                tmp_path = tmp.name
            try:
                subprocess.run(
                    ["diff", "-u",
                     "--label", f"repo/{label}",
                     "--label", f"system/{label}",
                     tmp_path, str(deployed)]
                )
            finally:
                os.unlink(tmp_path)
            try:
                answer = input("\n  Adopt this change? [y/N] ").strip().lower()
            except EOFError:
                answer = ""
            if answer not in ("y", "yes"):
                _info("Skipped.")
                continue

        if not dry:
            repo_dest.parent.mkdir(parents=True, exist_ok=True)
            repo_dest.write_bytes(incoming)
            changed_paths.append(repo_dest)

        _ok(f"{tag:9}  {label}  {C.DIM}← {deployed}{C.RESET}")
        counts[tag] += 1

    # Summary line
    parts = []
    if counts["added"]:
        parts.append(f"{counts['added']} added")
    if counts["updated"]:
        parts.append(f"{counts['updated']} updated")
    if counts["ok"]:
        parts.append(f"{counts['ok']} already up to date")
    if counts["missing"]:
        parts.append(f"{C.YELLOW}{counts['missing']} missing{C.RESET}")
    print()
    _ok("Done.  " + " · ".join(parts))

    if dry:
        _warn("(preview — nothing was changed)")
        return

    if not changed_paths:
        return

    # ── Commit ───────────────────────────────────────────────────────────────
    if args.no_commit:
        _info("Files written to repo — commit skipped (--no-commit).")
        _info("Stage and commit manually when ready.")
        return

    for p in changed_paths:
        _git("add", str(p))

    n = len(changed_paths)
    names = ", ".join(str(p.relative_to(REPO)) for p in changed_paths[:3])
    if n > 3:
        names += f", … (+{n - 3} more)"
    default_msg = f"adopt: {names}"
    msg = args.message or default_msg

    r = _git("commit", "-m", msg)
    if r.returncode == 0:
        _ok(f"Committed {n} file(s): {msg}")
    else:
        _warn("git commit failed — files are staged but not committed.")
        _warn(r.stderr.strip())


# ── add ───────────────────────────────────────────────────────────────────────


def _add_to_track(paths: List[Path]) -> None:
    """Append deployed paths to the 'track' list in config.json, no duplicates."""
    if not paths:
        return
    user_cfg = load_user_config()
    track = user_cfg.get("track", [])
    existing = set(track)
    home = Path.home()
    changed = False
    for p in paths:
        try:
            path_str = "~/" + str(p.relative_to(home))
        except ValueError:
            path_str = str(p)
        if path_str not in existing:
            track.append(path_str)
            existing.add(path_str)
            changed = True
    if changed:
        user_cfg["track"] = track
        save_user_config(user_cfg)


def cmd_add(args: argparse.Namespace) -> None:
    """Copy one or more files/directories into the repo and commit together."""
    config = load_config()
    dirs = active_dirs(config)

    added: List[Path] = []  # repo paths successfully written, for git add
    newly_tracked: List[Path] = []  # deployed paths to register in config track list

    def _add_one(file_path: Path) -> None:
        """Route and copy a single file into the repo tree."""
        matched_src: Optional[str] = None
        matched_dest: Optional[Path] = None
        for src_name, dest_dir in dirs:
            try:
                file_path.relative_to(dest_dir)
                matched_src = src_name
                matched_dest = dest_dir
                break
            except ValueError:
                continue

        if matched_src is None:
            (REPO / "home").mkdir(exist_ok=True)
            matched_src = "home"
            matched_dest = Path.home()

        rel = file_path.relative_to(matched_dest)  # type: ignore[arg-type]
        repo_file = REPO / matched_src / rel

        if repo_file.exists():
            if repo_file.read_bytes() == file_path.read_bytes():
                _dim(f"✓  already up to date: {matched_src}/{rel}")
                newly_tracked.append(file_path)
                return
            repo_file.write_bytes(file_path.read_bytes())
            _ok(f"Updated in repo: {matched_src}/{rel}")
            added.append(repo_file)
            newly_tracked.append(file_path)
            return
        repo_file.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(str(file_path), str(repo_file))
        _ok(f"Copied to repo: {matched_src}/{rel}")
        added.append(repo_file)
        newly_tracked.append(file_path)

    for raw in args.files:
        path = Path(raw).expanduser().resolve()

        if not path.exists():
            _err(f"Not found: {path}")
            continue

        if path.is_dir():
            files = sorted(p for p in path.rglob("*") if p.is_file() and not is_git(p))
            if not files:
                _warn(f"Empty directory, nothing to add: {path}")
                continue
            _info(f"Directory: {path}  ({len(files)} file(s) will be tracked)")
            for f in files:
                _dim(str(f.relative_to(path)))
            if sys.stdin.isatty():
                try:
                    answer = input(f"\n  Add these {len(files)} file(s)? [y/N] ").strip().lower()
                except EOFError:
                    answer = ""
                if answer not in ("y", "yes"):
                    _info("Skipped.")
                    continue
            for f in files:
                _add_one(f)
        else:
            _add_one(path)

    _add_to_track(newly_tracked)

    if not added:
        return

    for p in added:
        _git("add", str(p))

    # Stage updated config.json so the track list is committed alongside the files.
    cfg_file = REPO / CONFIG_NAME
    if cfg_file.exists():
        _git("add", str(cfg_file))

    n = len(added)
    names = ", ".join(str(p.relative_to(REPO)) for p in added[:3])
    if n > 3:
        names += f", ... (+{n - 3} more)"
    msg = args.message or f"add {names}"

    r = _git("commit", "-m", msg)
    if r.returncode == 0:
        _ok(f"Committed {n} file(s): {msg}")
    else:
        _warn("git commit failed — files are staged but not committed.")


# ── status ────────────────────────────────────────────────────────────────────


def cmd_status(args: argparse.Namespace) -> None:
    config = load_config()
    dirs = active_dirs(config)
    ignore = config.get("ignore", DEFAULT_IGNORE)
    plat, distro = detect_os()
    source_branch = git_current_branch()
    hostname = get_hostname()
    host_branch = f"host/{hostname}"

    _head("dtfls status")
    _info(f"Repo:         {REPO}")
    _info(f"Branch:       {source_branch}")
    _info(f"Platform:     {plat}" + (f" / {distro}" if distro else ""))
    exists_tag = (
        f"{C.GREEN}exists{C.RESET}"
        if git_branch_exists(host_branch)
        else f"{C.DIM}not yet created{C.RESET}"
    )
    _info(f"Host branch:  {host_branch}  ({exists_tag})")

    counts: Dict[str, int] = {"ok": 0, "differs": 0, "missing": 0}

    for src_name, dest_dir in dirs:
        src_label = "(repo root)" if src_name == "." else f"{src_name}/"
        _head(f"{src_label}  →  {dest_dir}")
        for repo_rel, rel_to_src in iter_branch_files(source_branch, src_name, ignore):
            target = dest_dir / rel_to_src
            content = git_show_bytes(source_branch, repo_rel)
            if content is None:
                continue
            if not target.exists():
                print(
                    f"  {C.RED}✗{C.RESET}  {rel_to_src}  {C.DIM}(not deployed){C.RESET}"
                )
                counts["missing"] += 1
            elif target.read_bytes() == content:
                _dim(f"✓  {rel_to_src}")
                counts["ok"] += 1
            else:
                print(
                    f"  {C.YELLOW}≠{C.RESET}  {rel_to_src}  {C.DIM}(deployed copy differs){C.RESET}"
                )
                counts["differs"] += 1

    print()
    parts = [
        f"{C.GREEN}{counts['ok']} in sync{C.RESET}",
        f"{C.YELLOW}{counts['differs']} differ{C.RESET}",
        f"{C.RED}{counts['missing']} not deployed{C.RESET}",
    ]
    print("  " + " · ".join(parts))
    if counts["differs"] or counts["missing"]:
        _info("Run  dtfls sync  to update.")


# ── diff ──────────────────────────────────────────────────────────────────────


def cmd_diff(args: argparse.Namespace) -> None:
    """Diff deployed files against repo versions using system diff."""
    config = load_config()
    dirs = active_dirs(config)
    ignore = config.get("ignore", DEFAULT_IGNORE)
    source_branch = git_current_branch()

    target_filter: Optional[Path] = (
        Path(args.file).expanduser().resolve() if args.file else None
    )

    found = False
    for src_name, dest_dir in dirs:
        for repo_rel, rel_to_src in iter_branch_files(source_branch, src_name, ignore):
            target = dest_dir / rel_to_src
            if target_filter and target != target_filter:
                continue
            if not target.is_file():
                continue
            content = git_show_bytes(source_branch, repo_rel)
            if content is None or target.read_bytes() == content:
                continue

            _head(f"diff: {rel_to_src}")
            suffix = target.suffix or ""
            with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as tmp:
                tmp.write(content)
                tmp_path = tmp.name
            try:
                subprocess.run(
                    [
                        "diff",
                        "-u",
                        "--label",
                        f"repo/{rel_to_src}",
                        "--label",
                        f"deployed/{rel_to_src}",
                        tmp_path,
                        str(target),
                    ]
                )
            finally:
                os.unlink(tmp_path)
            found = True

    if not found:
        _ok("No differences found.")


# ── branches ──────────────────────────────────────────────────────────────────


def cmd_branches(args: argparse.Namespace) -> None:
    hostname = get_hostname()

    _head(f"Host branches  {C.DIM}host/<hostname>{C.RESET}")
    host_branches = git_list_branches("host/")
    if host_branches:
        for b in sorted(host_branches):
            mine = (
                f"  {C.GREEN}← this machine{C.RESET}" if b == f"host/{hostname}" else ""
            )
            _info(f"{b}{mine}")
    else:
        _dim("none — create one with: git checkout -b host/<hostname>")

    _head(f"Backup branches  {C.DIM}backup/<hostname>-<timestamp>{C.RESET}")
    backup_branches = git_list_branches("backup/")
    if backup_branches:
        for b in sorted(backup_branches, reverse=True):
            mine = f"  {C.DIM}(this machine){C.RESET}" if hostname in b else ""
            _info(f"{b}{mine}")
    else:
        _dim("none")


# ── info ──────────────────────────────────────────────────────────────────────


def cmd_info(args: argparse.Namespace) -> None:
    config = load_config()
    dirs = active_dirs(config)
    plat, distro = detect_os()
    hostname = get_hostname()
    host_branch = f"host/{hostname}"

    _head("dtfls info")
    _info(f"Repo:         {REPO}")
    _info(f"Branch:       {git_current_branch()}")
    _info(f"Platform:     {plat}" + (f" / {distro}" if distro else ""))
    _info(f"Hostname:     {hostname}")
    exists_tag = (
        f"{C.GREEN}exists{C.RESET}"
        if git_branch_exists(host_branch)
        else f"{C.DIM}not yet created{C.RESET}"
    )
    _info(f"Host branch:  {host_branch}  ({exists_tag})")

    _head("Active source mappings")
    for src_name, dest_dir in dirs:
        src_label = "(repo root)" if src_name == "." else f"{src_name}/"
        _info(f"  {src_label}  →  {dest_dir}")

    _head("Ignore patterns")
    for pat in config.get("ignore", DEFAULT_IGNORE):
        _dim(pat)

    track = config.get("track", [])
    if track:
        _head(f"Tracked files  {C.DIM}({len(track)} entries){C.RESET}")
        tracked = resolve_tracked_files(config)
        for deployed, src_name, repo_dest in tracked:
            in_repo = repo_dest.is_file()
            on_disk = deployed.is_file()
            status = (
                f"{C.GREEN}✔ in repo{C.RESET}"
                if in_repo
                else f"{C.YELLOW}⚠ not yet adopted{C.RESET}"
            )
            disk_tag = "" if on_disk else f"  {C.RED}(not on disk){C.RESET}"
            _info(
                f"  {deployed}  →  {src_name}/{repo_dest.relative_to(REPO / src_name)}"
                f"  {status}{disk_tag}"
            )


# ── remove ────────────────────────────────────────────────────────────────────


def cmd_remove(args: argparse.Namespace) -> None:
    """
    Stop tracking one or more files or directories: delete from repo and commit.
    The deployed copy on the machine is left untouched.
    """
    config = load_config()
    dirs = active_dirs(config)

    removed_repo: List[Path] = []  # repo paths deleted, for git rm

    def _find_repo_path(deployed: Path) -> Optional[Tuple[str, Path, Path]]:
        """Return (src_name, rel, repo_path) for a deployed file path, or None."""
        matched_src: Optional[str] = None
        matched_dest: Optional[Path] = None
        for src_name, dest_dir in dirs:
            try:
                deployed.relative_to(dest_dir)
                matched_src = src_name
                matched_dest = dest_dir
                break
            except ValueError:
                continue
        if matched_src is None:
            return None
        rel = deployed.relative_to(matched_dest)  # type: ignore[arg-type]
        repo_path = REPO / matched_src / rel
        return src_name, rel, repo_path

    def _remove_one(deployed: Path) -> None:
        result = _find_repo_path(deployed)
        if result is None:
            _warn(f"Cannot map to repo: {deployed}")
            return
        src_name, rel, repo_path = result

        if not repo_path.exists():
            _warn(f"Not tracked in repo: {src_name}/{rel}")
            return

        if repo_path.is_dir():
            repo_files = [f for f in sorted(repo_path.rglob("*")) if f.is_file()]
            if not repo_files:
                _warn(f"Tracked dir is empty: {src_name}/{rel}")
                return
            for rf in repo_files:
                rf.unlink()
                removed_repo.append(rf)
            # Remove empty dirs left behind in the repo subtree.
            for dirpath in sorted(repo_path.rglob("*"), reverse=True):
                if dirpath.is_dir():
                    try:
                        dirpath.rmdir()
                    except OSError:
                        pass
            try:
                repo_path.rmdir()
            except OSError:
                pass
            _ok(f"Removed from repo: {src_name}/{rel}/  ({len(repo_files)} file(s))")
        else:
            repo_path.unlink()
            removed_repo.append(repo_path)
            _ok(f"Removed from repo: {src_name}/{rel}")

    for raw in args.files:
        path = Path(raw).expanduser().resolve()

        # Accept either the deployed path or the repo path.
        if path.is_relative_to(REPO):
            for src_name, dest_dir in dirs:
                src_root = REPO / src_name if src_name != "." else REPO
                try:
                    rel = path.relative_to(src_root)
                    path = dest_dir / rel
                    break
                except ValueError:
                    continue

        _remove_one(path)

    if not removed_repo:
        return

    _git("rm", "--cached", "--ignore-unmatch", *[str(p) for p in removed_repo])

    n = len(removed_repo)
    names = ", ".join(str(p.relative_to(REPO)) for p in removed_repo[:3])
    if n > 3:
        names += f", ... (+{n - 3} more)"
    msg = args.message or f"remove {names}"

    r = _git("commit", "-m", msg)
    if r.returncode == 0:
        _ok(f"Committed removal of {n} file(s): {msg}")
    else:
        _warn("git commit failed — deletions staged but not committed.")


# ── git pass-through ──────────────────────────────────────────────────────────


def cmd_pull(args: argparse.Namespace) -> None:
    _head(f"git pull  [{REPO.name}]")
    _git("pull", capture=False)


def cmd_push(args: argparse.Namespace) -> None:
    _head(f"git push  [{REPO.name}]")
    _git("push", capture=False)


# ── CLI ───────────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="dtfls",
        description=(
            "Portable dotfile sync. Manages your dotfiles repo "
            f"(default: {DEFAULT_REPO_DIR}, override with $DTFLS_REPO)."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
repo layout (convention, no config file required):
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
  dtfls info                              show config, mappings, track list status
        """,
    )
    sub = p.add_subparsers(dest="cmd", metavar="command")

    # adopt
    pa2 = sub.add_parser(
        "adopt",
        help="Copy all files listed in 'track' from system into repo",
    )
    pa2.add_argument(
        "--commit",
        action="store_true",
        help="Actually write files to repo and commit (default is preview only)",
    )
    pa2.add_argument(
        "--no-commit",
        action="store_true",
        help="With --commit: write files to repo but skip the git commit",
    )
    pa2.add_argument(
        "-m",
        "--message",
        metavar="<msg>",
        help="Override the auto-generated git commit message",
    )
    if FEATURE_INTERACTIVE:
        pa2.add_argument(
            "--interactive",
            action="store_true",
            help="For each changed file: show diff (repo vs system) and confirm before adopting",
        )

    # sync
    ps = sub.add_parser("sync", help="Copy dotfiles from repo to system")
    ps.add_argument(
        "--apply",
        action="store_true",
        help="Actually copy files to system (default is preview only)",
    )
    ps.add_argument(
        "--no-pull", action="store_true", help="Skip git pull before syncing"
    )
    ps.add_argument(
        "--host-branch",
        action="store_true",
        help=(
            "Select source branch automatically: uses host/<hostname> if it exists, "
            "otherwise falls back to the default branch (main/master). "
            "Also creates a backup/<hostname>-<timestamp> branch capturing the current "
            "deployed state of any file that would be overwritten."
        ),
    )
    ps.add_argument(
        "--no-save",
        action="store_true",
        help="With --host-branch: skip the backup snapshot branch",
    )

    # add
    pa = sub.add_parser(
        "add", help="Track one or more files: copy into repo and commit"
    )
    pa.add_argument("files", metavar="<file>", nargs="+")
    pa.add_argument("-m", "--message", metavar="<msg>", help="Git commit message")

    # remove
    pr = sub.add_parser(
        "remove",
        aliases=["rm"],
        help="Stop tracking files/dirs: delete from repo, leave deployed copy",
    )
    pr.add_argument("files", metavar="<file>", nargs="+")
    pr.add_argument("-m", "--message", metavar="<msg>", help="Git commit message")

    # status / diff / branches / info
    sub.add_parser("status", help="Show per-file sync state")
    pd = sub.add_parser("diff", help="Diff deployed files vs repo versions")
    pd.add_argument("file", metavar="<file>", nargs="?", help="Limit to one file")
    sub.add_parser("branches", help="List all host/ and backup/ branches")
    sub.add_parser("info", help="Show config, active mappings, host-branch status")

    # git pass-through
    sub.add_parser("pull", help="git pull the dotfile repo")
    sub.add_parser("push", help="git push the dotfile repo")

    return p


def main() -> None:
    global REPO

    parser = build_parser()
    args = parser.parse_args()

    dispatch = {
        "adopt": cmd_adopt,
        "sync": cmd_sync,
        "add": cmd_add,
        "remove": cmd_remove,
        "rm": cmd_remove,
        "status": cmd_status,
        "diff": cmd_diff,
        "branches": cmd_branches,
        "info": cmd_info,
        "pull": cmd_pull,
        "push": cmd_push,
    }

    if args.cmd not in dispatch:
        parser.print_help()
        sys.exit(0)

    REPO = resolve_repo()
    ensure_config()

    try:
        dispatch[args.cmd](args)
    except KeyboardInterrupt:
        print()
        _warn("Interrupted.")
        sys.exit(130)


if __name__ == "__main__":
    main()
