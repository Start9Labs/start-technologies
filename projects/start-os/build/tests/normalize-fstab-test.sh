#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname "${BASH_SOURCE[0]}")/../../../..")
NORMALIZER="$ROOT/projects/start-os/build/lib/scripts/normalize-fstab"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/bin"

cat > "$TMP/bin/findmnt" <<'EOF2'
#!/bin/bash
set -eu
[ "${IDENTITY_FAIL:-0}" -ne 1 ] || exit 2
case "${!#}" in
    /media/startos/root) printf '/dev/sdb3\n' ;;
    /boot)
        if [ "${BOOT_MISMATCH:-0}" -eq 1 ]; then
            printf '/dev/sdc2\n'
        else
            printf '/dev/sdb2\n'
        fi
        ;;
    /boot/efi)
        [ "${NO_EFI:-0}" -ne 1 ] || exit 1
        printf '/dev/sdb1\n'
        ;;
    *) exit 1 ;;
esac
EOF2

cat > "$TMP/bin/lsblk" <<'EOF2'
#!/bin/bash
set -eu
[ "$2" = PKNAME ] || exit 1
name=${!#}
name=${name#/dev/}
printf '%s\n' "${name%%[0-9]*}"
EOF2

cat > "$TMP/bin/blkid" <<'EOF2'
#!/bin/bash
set -eu
if [ "$1" = -p ]; then
    device=${!#}
    [ "${FAIL_DEVICE:-}" != "$device" ] || exit 2
    case "$device" in
        /dev/sdb1) printf 'efi-id\n' ;;
        /dev/sdb2) printf 'boot-id\n' ;;
        *) exit 2 ;;
    esac
    exit
fi

case "${2#PARTUUID=}" in
    efi-id) device=/dev/sdb1 ;;
    boot-id) device=/dev/sdb2 ;;
    *) exit 2 ;;
esac
if [ "${DUPLICATE_ID:-0}" -eq 1 ]; then
    printf '%s\n/dev/sdc9\n' "$device"
elif [ "${WRONG_ID_TARGET:-0}" -eq 1 ]; then
    printf '/dev/sdc9\n'
else
    printf '%s\n' "$device"
fi
EOF2

cat > "$TMP/bin/readlink" <<'EOF2'
#!/bin/bash
printf '%s\n' "${!#}"
EOF2

cat > "$TMP/bin/sync" <<'EOF2'
#!/bin/bash
exit 0
EOF2

chmod +x "$TMP/bin/"*
export PATH="$TMP/bin:$PATH"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

assert_same() {
    cmp -s -- "$1" "$2" || {
        diff -u -- "$1" "$2" >&2 || true
        fail "$3"
    }
}

assert_fails_unchanged() {
    local file=$1 before=$TMP/before
    cp -- "$file" "$before"
    if "$NORMALIZER" "$file" >/dev/null 2>&1; then
        fail "$2 succeeded"
    fi
    assert_same "$before" "$file" "$2 changed fstab"
}

fstab=$TMP/fstab
expected=$TMP/expected
printf '%s' '# keep comment
/dev/sda2	/boot	vfat	umask=0077	0	2
/dev/sda1 /boot/efi vfat defaults 0 1
/dev/sda3 / btrfs defaults 0 1
/dev/sdz1 /srv ext4 defaults 0 2' > "$fstab"
printf '%s' '# keep comment
PARTUUID=boot-id	/boot	vfat	umask=0077	0	2
PARTUUID=efi-id /boot/efi vfat defaults 0 1
/dev/sda3 / btrfs defaults 0 1
/dev/sdz1 /srv ext4 defaults 0 2' > "$expected"
chmod 0640 "$fstab"
"$NORMALIZER" "$fstab"
assert_same "$expected" "$fstab" 'normalization mismatch'
[ "$(stat -c %a "$fstab")" = 640 ] || fail 'mode changed'
[ "$(tail -c 1 "$fstab" | od -An -t x1 | tr -d '[:space:]')" != 0a ] ||
    fail 'final newline added'
cp -- "$fstab" "$TMP/normalized"
"$NORMALIZER" "$fstab"
assert_same "$TMP/normalized" "$fstab" 'second run changed fstab'

mkdir "$TMP/real"
printf '/dev/sda2 /boot vfat defaults 0 2\n' > "$TMP/real/fstab"
ln -s real/fstab "$TMP/fstab-link"
"$NORMALIZER" "$TMP/fstab-link"
[ -L "$TMP/fstab-link" ] || fail 'fstab symlink replaced'
grep -q '^PARTUUID=boot-id ' "$TMP/real/fstab" || fail 'symlink target not normalized'

printf '/dev/sda2 /boot vfat defaults 0 2\n/dev/sda1 /boot/efi vfat defaults 0 1\n' > "$fstab"
FAIL_DEVICE=/dev/sdb2 assert_fails_unchanged "$fstab" 'missing PARTUUID'
BOOT_MISMATCH=1 assert_fails_unchanged "$fstab" 'boot on another disk'
NO_EFI=1 assert_fails_unchanged "$fstab" 'unmounted EFI'
DUPLICATE_ID=1 assert_fails_unchanged "$fstab" 'duplicate PARTUUID'
WRONG_ID_TARGET=1 assert_fails_unchanged "$fstab" 'mismatched PARTUUID'

printf 'PARTUUID=boot-id /boot vfat defaults 0 2\n/dev/sda3 / btrfs defaults 0 1\n' > "$fstab"
IDENTITY_FAIL=1 "$NORMALIZER" "$fstab"

printf 'normalize-fstab tests passed\n'
