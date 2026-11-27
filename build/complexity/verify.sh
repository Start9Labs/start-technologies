#!/bin/bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LOG="${LOG:-build/complexity/history.tsv}"
[ -x "$HERE/bin/bca" ] || "$HERE/fetch-tool.sh"
export BCA="$HERE/bin/bca"

fresh="$(node "$HERE/census.mjs" --root . --totals)"
last="$(tail -1 "$LOG" | cut -f3-7)"
if [ "$last" != "$fresh" ]; then
  echo "complexity: the last row of $LOG does not describe this tree." >&2
  echo "  tree:     $fresh" >&2
  echo "  last row: $last" >&2
  echo "Run 'make complexity-record' and commit the result." >&2
  exit 1
fi
echo "complexity: the last row describes this tree ($fresh)"
