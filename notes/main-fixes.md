# Fixes to the Python implementation

Found while reviewing `main` at `4e63fd5`, ahead of the Rust port, so the port
copies corrected behaviour rather than bugs. Each fix has a regression test in
`tests/test_add_remove.py`.

| # | Issue | Effect | Fix |
|---|---|---|---|
| 1 | `_add_to_track` built `set(track)` | `dtfls add` crashed with `TypeError: unhashable type: 'dict'` once `config.json` had any object entry such as `{"path": "~/.zprofile", "os": "darwin"}` | Compare against each entry's path (string, or the object's `"path"`) |
| 2 | `add` / `remove` routed to the **first** (most general) matching dir; `adopt` / `info` / `resolve_tracked_files` use the **most specific** | On Fedora with `home/` and `home.linux/`, `add ~/.bashrc` wrote `home/.bashrc`, but `adopt` later wrote `home.linux/.bashrc`: two diverging copies | New shared `route()` (most specific wins) used by `add` and `resolve_tracked_files`. `remove` prefers the most specific dir that actually holds the file, falling back to the most specific covering dir |
| 3 | `Path.is_relative_to` is Python 3.9+, but `pyproject.toml` declares `>=3.8` | `dtfls remove` crashed on 3.8 | `_is_relative_to()` shim |
| 4 | `add` on a file outside every mapped destination | Uncaught `ValueError` traceback | Clean error message, file skipped |
| 5 | Ignore patterns with a trailing `/` (the `"secrets/"` example in `--help`) were matched with `fnmatch` against a bare name, so they never matched | Files under an ignored directory were synced anyway | Strip the trailing `/` before matching |
| 6 | README drift | Said `.dtfls.json` (code uses `config.json`); documented `adopt --dry-run` (preview is now the default); listed `sync` twice | README command list rewritten to match the CLI |

## Retracted

The initial review also flagged `remove`'s `_find_repo_path` for returning the
leaked loop variable `src_name` instead of `matched_src`. Because the loop
`break`s on its first match, the two were always equal, so the label was
correct. The function was rewritten for fix 2 anyway, which removes the
fragility.

## Not changed (noted for later)

- `remove` deletes the file from the repo but leaves its entry in
  `config.json`'s `track` list, so `adopt` will re-add it. Probably wants to
  untrack too, but that's a behaviour change worth deciding on deliberately.
