# Branch report (2026-10-30)

Snapshot of every branch on `origin`, compared against `main` at `4e63fd5`
("invert --dry-run for adopt & sync").

| Branch | Ahead of `main` | Behind `main` | State |
|---|---|---|---|
| `main` | — | — | `4e63fd5` |
| `feature/config-added-auto-tracking` | 0 | 0 | Same commit as `main`. Fully merged. |
| `feature/walk-dirs-adding-children` | 0 | 2 | `cec3569` is in `main`. Fully merged. |
| `claude/dtfls-package-plan-7gt75d` | 0 | 4 | In `main` via merge commit `2b8c048` (PR #3). Fully merged. |
| `claude/dtfls-homebrew-pypi-setup` | 1 (`d6c4360`) | 6 | Only unmerged work. Draft PR #4. |

## How they differ

- **The three merged branches** add nothing. Since `2b8c048`, `main` has only
  gained `cli.py` changes: directory walking in `add`, a `config.json` that is
  created on first run and maintained by `add`, and preview-by-default for
  `adopt` / `sync`.
- **PR #4** (`claude/dtfls-homebrew-pypi-setup`) is release plumbing only:
  - enables the PyPI job in `release.yml`;
  - adds a `homebrew` job that bumps `wrnkt/homebrew-dtfls` with
    `mislav/bump-homebrew-formula-action`;
  - adds `RELEASING.md` and `packaging/homebrew/dtfls.rb` (a Python
    virtualenv formula).
  - It does not touch `cli.py`, and a test merge into `main` is conflict-free.

## Recommended order / decisions

1. Delete the three fully merged branches — no merge needed.
2. **Hold PR #4** as a reference for the Rust port. Its PyPI job and Python
   formula become obsolete once the tool is Rust; the tap setup and the
   `HOMEBREW_TAP_TOKEN` instructions in `RELEASING.md` carry over.

## Actions taken

- PR #4's base was `claude/dtfls-package-plan-7gt75d` (a branch whose own PR
  was closed). Deleting that branch would have auto-closed PR #4, so PR #4 was
  **retargeted to `main`** first. It is otherwise untouched and still a draft.
- **Branch deletion is still pending.** The session's git proxy rejected
  `git push origin --delete …` with HTTP 403 for all three branches (this
  session may only push `claude/*` branches it creates). Delete them from the
  GitHub UI, or locally with:

  ```
  git push origin --delete \
    feature/config-added-auto-tracking \
    feature/walk-dirs-adding-children \
    claude/dtfls-package-plan-7gt75d
  ```
