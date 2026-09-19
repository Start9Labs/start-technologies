#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
RESIZER="$ROOT/projects/start-os/build/image-recipe/raspberrypi/img/usr/lib/startos/scripts/init_resize.sh"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/bin"

cat > "$TMP/bin/mount" <<'EOF2'
#!/bin/bash
printf '%s\n' "$*" >> "$MOUNT_LOG"
[ "${MOUNT_FAIL:-0}" -ne 1 ]
EOF2

cat > "$TMP/bin/btrfs" <<'EOF2'
#!/bin/bash
printf '%s\n' "$*" >> "$BTRFS_LOG"
[ "${BTRFS_FAIL:-0}" -ne 1 ]
EOF2

chmod +x "$TMP/bin/"*
export PATH="$TMP/bin:$PATH"
export MOUNT_LOG="$TMP/mount.log"
export BTRFS_LOG="$TMP/btrfs.log"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

FUNCTIONS="$TMP/functions.sh"
if ! awk '/^mkdir -p \/run\/systemd$/ { found=1; exit } { print } END { if (!found) exit 1 }' "$RESIZER" > "$FUNCTIONS"; then
    fail 'could not isolate init_resize functions'
fi
source "$FUNCTIONS"

grow_root_filesystem
[ "$(cat "$MOUNT_LOG")" = '/ -o remount,rw' ] || fail 'root was not remounted read-write'
[ "$(cat "$BTRFS_LOG")" = 'filesystem resize max /media/startos/config' ] ||
    fail 'btrfs was not resized through the writable bind mount'

: > "$BTRFS_LOG"
FAIL_REASON=
BTRFS_FAIL=1
export BTRFS_FAIL
if grow_root_filesystem >/dev/null; then
    fail 'btrfs resize failure succeeded'
fi
[ "$FAIL_REASON" = 'Root filesystem resize failed' ] || fail 'btrfs resize failure reason missing'

: > "$BTRFS_LOG"
FAIL_REASON=
MOUNT_FAIL=1
export MOUNT_FAIL
if grow_root_filesystem >/dev/null; then
    fail 'root remount failure succeeded'
fi
[ "$FAIL_REASON" = 'Root remount failed' ] || fail 'root remount failure reason missing'
[ ! -s "$BTRFS_LOG" ] || fail 'btrfs resize ran after remount failure'

get_variables() {
    ROOT_PART_DEV=/dev/root1
    ROOT_DEV=/dev/root
    ROOT_PART_NUM=1
    LAST_PART_NUM=1
    ROOT_PART_END=100
    TARGET_END=200
    ROOT_PART_START=1
    DATA_PART_START=
}
check_variables() { return 0; }
sfdisk() { return 0; }
partx() { return 0; }
grow_root_filesystem() {
    FAIL_REASON='Root filesystem resize failed'
    return 1
}
systemd-machine-id-setup() {
    FAIL_REASON='main continued after resize failure'
    return 1
}

FAIL_REASON=
if main >/dev/null; then
    fail 'main accepted a root filesystem resize failure'
fi
[ "$FAIL_REASON" = 'Root filesystem resize failed' ] || fail "$FAIL_REASON"

printf 'init-resize tests passed\n'
