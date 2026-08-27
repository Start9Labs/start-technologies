#!/bin/bash
# Cognitive-complexity census. `census` prints the totals and the worst 25; `diff <base>`
# prints this branch's delta against its merge-base; `check <body-file> <base>` fails when
# a PR body's block is absent, unanswered, or disagrees with a fresh run.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
export RCA="${RCA:-$HERE/bin/rust-code-analysis-cli}"

[ -x "$RCA" ] || "$HERE/fetch-tool.sh"

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
  check)
    body="$2"; base="${3:-origin/master}"
    fresh="$(mktemp)"; trap 'rm -f "$fresh"' EXIT
    "$0" diff "$base" > "$fresh"
    python3 "$HERE/check.py" "$body" "$fresh"
    ;;
  *) echo "usage: census.sh {census|top [n]|diff <base>|check <body> <base>}" >&2; exit 2 ;;
esac
