#!/usr/bin/env bash
# Run the same scenario against the Python and Rust implementations and diff
# their output. Used during the port; delete at cutover.
#
#   scripts/parity.sh [python-cmd] [rust-binary]
#
# Defaults: `python3 -c 'from dtfls.cli import main; main()'` (needs the
# package importable, e.g. `pip install -e .`) and target/debug/dtfls.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
py_cmd=${1:-"python3 -c 'from dtfls.cli import main; main()'"}
rs_bin=${2:-"$root/target/debug/dtfls"}
work=$(mktemp -d)
trap '[ -n "${KEEP:-}" ] && echo "kept $work" || rm -rf "$work"' EXIT

scenario() {
  local impl=$1 base=$2
  export HOME="$base/home" DTFLS_REPO="$base/repo" NO_COLOR=1
  export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t \
    GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
  mkdir -p "$HOME/.config/nvim/lua"
  cd "$HOME"
  printf 'echo hi\n' > .bashrc
  printf 'set nu\n' > .vimrc
  printf 'x\n' > .config/nvim/init.lua
  printf 'y\n' > .config/nvim/lua/a.lua

  run() { echo "\$ dtfls $*"; eval "$impl" '"$@"' 2>&1 < /dev/null || echo "[exit $?]"; }

  run info
  mkdir -p "$DTFLS_REPO/home"
  run add ~/.bashrc
  run add ~/.config/nvim -m "nvim"
  run status
  printf 'echo changed\n' > .bashrc
  run diff
  run diff ~/.bashrc
  run sync --no-pull
  run sync --no-pull --apply
  run status
  printf '{"track": ["~/.bashrc", "~/.vimrc", "~/.gone", {"path": "~/.zprofile", "os": "darwin"}], "ignore": ["secrets/"]}\n' \
    > "$DTFLS_REPO/config.json"
  run info
  run adopt
  run adopt --commit
  run branches
  run rm ~/.bashrc
  run rm "$DTFLS_REPO/home/.config/nvim"
  run status
  echo "--- git log"
  git -C "$DTFLS_REPO" log --format=%s
}

mkdir -p "$work/py" "$work/rs"
(scenario "$py_cmd" "$work/py") | sed "s#$work/py#<tmp>#g" > "$work/py.out"
(scenario "$rs_bin" "$work/rs") | sed "s#$work/rs#<tmp>#g" > "$work/rs.out"

if diff -u --label python "$work/py.out" --label rust "$work/rs.out"; then
  echo "parity: identical output"
else
  exit 1
fi
