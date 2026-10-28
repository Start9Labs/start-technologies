#!/usr/bin/env bash
# Type-checks service packages against the built dist/ and against a published
# SDK version, and fails on any error the build introduces.
#
#   ./check-consumers.sh [-p <previous-version>] <package-dir>...
#
# A package dir is a checkout with `startos/` and an installed `node_modules`.
# <previous-version> defaults to the latest version on npm.
set -euo pipefail

dist=$(cd "$(dirname "$0")" && pwd)/dist
[ -f "$dist/package.json" ] || { echo "dist/ is missing; run make bundle" >&2; exit 2; }

previous=
if [ "${1:-}" = -p ]; then previous=$2; shift 2; fi
[ $# -gt 0 ] || { sed -n '2,8s/^# \{0,1\}//p' "$0" >&2; exit 2; }
previous=${previous:-$(npm view @start9labs/start-sdk version)}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
(cd "$work" && npm pack -q "@start9labs/start-sdk@$previous" >/dev/null && tar xzf ./*.tgz)

check() {
  local label=$1 sdk=$2 pkg=$3 name cfg log
  name=$(echo "$pkg" | tr / _)
  cfg=$work/$label/$name.tsconfig.json
  mkdir -p "$work/$label"
  cat >"$cfg" <<EOF
{ "extends": "$pkg/tsconfig.json",
  "compilerOptions": { "noEmit": true, "rootDir": "/", "typeRoots": ["$pkg/node_modules/@types"],
    "paths": { "@start9labs/start-sdk": ["$sdk/lib/index.d.ts"], "@start9labs/start-sdk/*": ["$sdk/*"] } },
  "include": ["$pkg/startos/**/*.ts"] }
EOF
  log=$work/$label/$name.log
  if ! "$pkg/node_modules/.bin/tsc" -p "$cfg" >"$log" 2>&1; then
    if ! grep -q 'error TS' "$log"; then
      echo "ERROR $label $pkg: type-check did not complete" >&2
      cat "$log" >&2
      return 2
    fi
  fi
  grep 'error TS' "$log" |
    sed -E 's|^.*/startos/|startos/|; s|\(([0-9]+),[0-9]+\)||; s|"[^"]*/(lib/[^"]*)"|"\1"|g' |
    sort -u >"$work/$label/$name.txt" || true
}

failed=0
for pkg in "$@"; do
  pkg=$(cd "$pkg" && pwd)
  if [ ! -x "$pkg/node_modules/.bin/tsc" ]; then
    echo "SKIP $pkg: no node_modules"
    continue
  fi
  check previous "$work/package" "$pkg" &
  previous_pid=$!
  check candidate "$dist" "$pkg" &
  candidate_pid=$!
  status=0
  wait "$previous_pid" || status=2
  wait "$candidate_pid" || status=2
  [ "$status" -eq 0 ] || exit "$status"
  name=$(echo "$pkg" | tr / _)
  introduced=$(comm -13 "$work/previous/$name.txt" "$work/candidate/$name.txt")
  if [ -n "$introduced" ]; then
    failed=$((failed + 1))
    echo "FAIL $pkg"
    echo "$introduced" | sed 's/^/  /'
  fi
done

echo "$failed of $# packages gained type errors against $previous"
[ "$failed" -eq 0 ]
