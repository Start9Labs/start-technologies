#!/bin/bash
# Fails when the log's last row does not describe the working tree. Proves the
# author ran the census on what they are actually shipping — it says nothing
# about whether the numbers went up.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LOG="${LOG:-build/complexity/history.tsv}"
[ -x "$HERE/bin/bca" ] || "$HERE/fetch-tool.sh"
export BCA="$HERE/bin/bca"

read -r functions cognitive cyclomatic sloc over25 < <(
  python3 "$HERE/census.py" --root . --json \
    | python3 -c 'import json,sys; t=json.load(sys.stdin)["totals"]; print(t["functions"],t["cognitive"],t["cyclomatic"],t["sloc"],t["over25"])'
)
fresh="$functions	$cognitive	$cyclomatic	$sloc	$over25"
if ! cut -f3-7 "$LOG" | grep -qxF "$fresh"; then
  echo "complexity: no row in $LOG describes this tree." >&2
  echo "  tree:     $fresh" >&2
  echo "  last row: $(tail -1 "$LOG" | cut -f3-7)" >&2
  echo "Run 'make complexity-record' and commit the result." >&2
  exit 1
fi
echo "complexity: the log describes this tree ($fresh)"
