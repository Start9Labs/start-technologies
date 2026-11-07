#!/bin/bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
export BCA="${BCA:-$HERE/bin/bca}"

[ -x "$BCA" ] || "$HERE/fetch-tool.sh"

case "${1:-census}" in
  census) node "$HERE/census.mjs" --root . ;;
  top)    node "$HERE/census.mjs" --root . --top "${2:-25}" ;;
  diff)
    base="${2:-origin/master}"
    mb="$(git merge-base "$base" HEAD)"
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    git archive "$mb" | tar -x -C "$tmp"
    node "$HERE/census.mjs" --root "$tmp" --json > "$tmp/.base.json"
    node "$HERE/census.mjs" --root .      --json > "$tmp/.head.json"
    node "$HERE/delta.mjs" "$tmp/.base.json" "$tmp/.head.json" "$mb"
    ;;
  *) echo "usage: census.sh {census|top [n]|diff <base>}" >&2; exit 2 ;;
esac
