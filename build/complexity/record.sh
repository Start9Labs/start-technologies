#!/bin/bash
# Appends one row for HEAD to the complexity log. Only master CI runs this, so a
# pull request never edits the file and never conflicts on it.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LOG="${1:-build/complexity/history.tsv}"
[ -x "$HERE/bin/bca" ] || "$HERE/fetch-tool.sh"
read -r functions cognitive cyclomatic sloc over25 < <(
  BCA="$HERE/bin/bca" python3 "$HERE/census.py" --root . --json \
    | python3 -c 'import json,sys; t=json.load(sys.stdin)["totals"]; print(t["functions"],t["cognitive"],t["cyclomatic"],t["sloc"],t["over25"])'
)
[ -s "$LOG" ] || printf 'commit\tdate\tfunctions\tcognitive\tcyclomatic\tsloc\tover25\n' > "$LOG"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$(git rev-parse --short HEAD)" "$(git log -1 --format=%cs)" \
  "$functions" "$cognitive" "$cyclomatic" "$sloc" "$over25" >> "$LOG"
