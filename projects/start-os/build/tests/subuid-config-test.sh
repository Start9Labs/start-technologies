#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
POSTINST="$ROOT/projects/start-os/debian/postinst"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/etc"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

if ! awk '/^SLOT_IDS_START=/ { copying=1 } copying { print } copying && /^done$/ && ++blocks == 2 { found=1; exit } END { if (!found) exit 1 }' "$POSTINST" |
    sed "s|/etc/|$TMP/etc/|g" > "$TMP/subuid-install.sh"; then
    fail 'could not isolate subordinate id configuration'
fi

for ids in subuid subgid; do
    printf 'root:100000:65536\nkiosk:165536:65536\nstart9:231072:65536\nfar:90000000:65536\n' > "$TMP/etc/$ids"
done
printf '#SUB_UID_MIN\t\t   100000\n#SUB_GID_MIN\t\t   100000\nUID_MIN 1000\n' > "$TMP/etc/login.defs"
sh "$TMP/subuid-install.sh"
sh "$TMP/subuid-install.sh"

for ids in subuid subgid; do
    expected=$'kiosk:90065536:65536\nstart9:90131072:65536\nfar:90000000:65536\nroot:65536:67108864'
    [ "$(cat "$TMP/etc/$ids")" = "$expected" ] || fail "$ids is $(cat "$TMP/etc/$ids")"
done
grep -qx 'SUB_UID_MIN 67174400' "$TMP/etc/login.defs" || fail 'SUB_UID_MIN is not above the slot span'
grep -qx 'SUB_GID_MIN 67174400' "$TMP/etc/login.defs" || fail 'SUB_GID_MIN is not above the slot span'
[ "$(grep -c SUB_UID_MIN "$TMP/etc/login.defs")" = 1 ] || fail 'SUB_UID_MIN is duplicated'
grep -qx 'UID_MIN 1000' "$TMP/etc/login.defs" || fail 'unrelated login.defs settings changed'

printf 'subordinate id configuration tests passed\n'
