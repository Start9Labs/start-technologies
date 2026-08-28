#!/bin/bash
# Prints this branch's complexity delta and appends its totals to the history log.
# The log is merge=union, so two branches appending different rows never conflict.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LOG="${LOG:-build/complexity/history.tsv}"
BASE="${BASE:-origin/master}"
[ -x "$HERE/bin/bca" ] || "$HERE/fetch-tool.sh"
export BCA="$HERE/bin/bca"

"$HERE/census.sh" diff "$BASE" || true

read -r functions cognitive cyclomatic sloc over25 < <(
  python3 "$HERE/census.py" --root . --json \
    | python3 -c 'import json,sys; t=json.load(sys.stdin)["totals"]; print(t["functions"],t["cognitive"],t["cyclomatic"],t["sloc"],t["over25"])'
)
[ -s "$LOG" ] || printf 'commit\tdate\tfunctions\tcognitive\tcyclomatic\tsloc\tover25\n' > "$LOG"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$(git rev-parse --short HEAD)" "$(git log -1 --format=%cs)" \
  "$functions" "$cognitive" "$cyclomatic" "$sloc" "$over25" >> "$LOG"
echo
echo "recorded $functions/$cognitive/$cyclomatic/$sloc/$over25 to $LOG"
