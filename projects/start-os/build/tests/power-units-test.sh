#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
UNITS="$TMP/usr/lib/systemd/system"
mkdir -p "$UNITS" "$TMP/usr/bin" "$TMP/bin"

# verify checks executables without running them.
printf '#!/bin/sh\nexit 1\n' > "$TMP/usr/bin/start-cli"
cp "$TMP/usr/bin/start-cli" "$TMP/bin/true"
chmod +x "$TMP/usr/bin/start-cli" "$TMP/bin/true"
printf '[Unit]\nDescription=Test target\nDefaultDependencies=no\n' > "$UNITS/sysinit.target"

for action in restart shutdown; do
    unit="startos-$action.service"
    cp "$ROOT/projects/start-os/$unit" "$UNITS/$unit"
    if ! SYSTEMD_LOG_LEVEL=debug systemd-analyze verify --man=no --root="$TMP" "$unit" > "$TMP/parsed" 2>&1; then
        cat "$TMP/parsed" >&2
        exit 1
    fi
    awk '/ExecStop:/ { getline; print }' "$TMP/parsed" |
        grep -Eq "^[[:space:]]*Command Line: /usr/bin/start-cli server $action --force$" || {
        printf 'FAIL: %s must interrupt backups during systemd teardown\n' "$unit" >&2
        cat "$TMP/parsed" >&2
        exit 1
    }
done

printf 'power unit tests passed\n'
