#!/bin/bash
# Cognitive-complexity census. `census` prints the totals and the worst 25; `top` the worst N;
# `diff <base>` what this branch did to them. Nothing here exits non-zero on a number.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
export BCA="${BCA:-$HERE/bin/bca}"

[ -x "$BCA" ] || "$HERE/fetch-tool.sh"

case "${1:-census}" in
  census) python3 "$HERE/census.py" --root . ;;
  top)    python3 "$HERE/census.py" --root . --top "${2:-25}" ;;
  diff)
    base="${2:-origin/master}"
    mb="$(git merge-base "$base" HEAD)"
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    git archive "$mb" | tar -x -C "$tmp"
    python3 "$HERE/census.py" --root "$tmp" --json > "$tmp/.base.json"
    python3 "$HERE/census.py" --root .      --json > "$tmp/.head.json"
    python3 "$HERE/delta.py" "$tmp/.base.json" "$tmp/.head.json" "$mb"
    ;;
  *) echo "usage: census.sh {census|top [n]|diff <base>}" >&2; exit 2 ;;
esac
