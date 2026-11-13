#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
POSTINST="$ROOT/projects/start-os/debian/postinst"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/etc/sysctl.d"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

if ! awk '/^cat > \/etc\/sysctl\.d\/97-startos\.conf <<EOF$/ { copying=1 } copying { print } copying && /^EOF$/ { found=1; exit } END { if (!found) exit 1 }' "$POSTINST" |
    sed "s|/etc/|$TMP/etc/|g" > "$TMP/sysctl-install.sh"; then
    fail 'could not isolate sysctl configuration'
fi
sh "$TMP/sysctl-install.sh"
grep -qx 'fs.inotify.max_user_instances=8192' "$TMP/etc/sysctl.d/97-startos.conf" || fail 'fs.inotify.max_user_instances is not 8192'

printf 'sysctl configuration tests passed\n'
