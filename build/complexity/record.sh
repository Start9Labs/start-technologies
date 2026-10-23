#!/bin/bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LOG="${LOG:-build/complexity/history.tsv}"
BASE="${BASE:-origin/master}"
[ -x "$HERE/bin/bca" ] || "$HERE/fetch-tool.sh"
export BCA="$HERE/bin/bca"

"$HERE/census.sh" diff "$BASE" || true

fresh="$(node "$HERE/census.mjs" --root . --totals)"
read -r functions cognitive cyclomatic sloc over25 <<< "$fresh"
[ -s "$LOG" ] || printf 'commit\tdate\tfunctions\tcognitive\tcyclomatic\tsloc\tover25\n' > "$LOG"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$(git rev-parse --short HEAD)" "$(git log -1 --format=%cs)" \
  "$functions" "$cognitive" "$cyclomatic" "$sloc" "$over25" >> "$LOG"
echo
echo "recorded $functions/$cognitive/$cyclomatic/$sloc/$over25 to $LOG"
