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
target=${!#}
root_number=${ROOT_NUMBER:-3}
case "$target" in
    /|/media/startos/root) printf '/dev/sdb%s\n' "$root_number" ;;
    /boot)
        if [ "${BOOT_MISMATCH:-0}" -eq 1 ]; then
            printf '/dev/sdc2\n'
        else
            printf '/dev/sdb%s\n' "$((root_number - 1))"
        fi
        ;;
    /boot/efi) printf '/dev/sdb1\n' ;;
    *) exit 1 ;;
esac
EOF2

cat > "$TMP/bin/lsblk" <<'EOF2'
#!/bin/bash
set -eu
root_number=${ROOT_NUMBER:-3}
case "$2" in
    PKNAME) printf 'sdb\n' ;;
    PARTN) printf '%s\n' "$root_number" ;;
    PATH,PARTN)
        printf '/dev/sdb  \n'
        for ((number = 1; number <= root_number; number++)); do
            printf '/dev/sdb%s %s\n' "$number" "$number"
        done
        ;;
    *) exit 1 ;;
esac
EOF2

cat > "$TMP/bin/blkid" <<'EOF2'
#!/bin/bash
set -eu
root_number=${ROOT_NUMBER:-3}
if [ "$1" = -p ]; then
    device=${!#}
    [ "${FAIL_DEVICE:-}" != "$device" ] || exit 2
    case "$root_number:$device" in
        3:/dev/sdb1) printf 'efi-id\n' ;;
        3:/dev/sdb2) printf 'boot-id\n' ;;
        2:/dev/sdb1) printf 'boot-id\n' ;;
        *) exit 2 ;;
    esac
    exit
fi

id=${2#PARTUUID=}
case "$id" in
    efi-id) device=/dev/sdb1 ;;
    boot-id) device="/dev/sdb$((root_number - 1))" ;;
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
assert_same "$expected" "$fstab" 'GPT normalization mismatch'
[ "$(stat -c %a "$fstab")" = 640 ] || fail 'mode changed'
[ "$(tail -c 1 "$fstab" | od -An -t x1 | tr -d '[:space:]')" != 0a ] ||
    fail 'final newline added'
cp -- "$fstab" "$TMP/normalized"
"$NORMALIZER" "$fstab"
assert_same "$TMP/normalized" "$fstab" 'second run changed fstab'

printf '/dev/sda1 /boot vfat defaults 0 2\n/dev/sda2 / ext4 defaults 0 1\n' > "$fstab"
printf 'PARTUUID=boot-id /boot vfat defaults 0 2\n/dev/sda2 / ext4 defaults 0 1\n' > "$expected"
ROOT_NUMBER=2 "$NORMALIZER" --legacy "$fstab"
assert_same "$expected" "$fstab" 'legacy MBR normalization mismatch'

printf '/dev/sda2 /boot vfat defaults 0 2\n/dev/sda1 /boot/efi vfat defaults 0 1\n/dev/sda3 / ext4 defaults 0 1\n' > "$fstab"
printf 'PARTUUID=boot-id /boot vfat defaults 0 2\nPARTUUID=efi-id /boot/efi vfat defaults 0 1\n/dev/sda3 / ext4 defaults 0 1\n' > "$expected"
"$NORMALIZER" --legacy "$fstab"
assert_same "$expected" "$fstab" 'legacy GPT normalization mismatch'

mkdir "$TMP/real"
printf '/dev/sda2 /boot vfat defaults 0 2\n' > "$TMP/real/fstab"
ln -s real/fstab "$TMP/fstab-link"
"$NORMALIZER" "$TMP/fstab-link"
[ -L "$TMP/fstab-link" ] || fail 'fstab symlink replaced'
grep -q '^PARTUUID=boot-id ' "$TMP/real/fstab" || fail 'symlink target not normalized'

printf '/dev/sda2 /boot vfat defaults 0 2\n/dev/sda3 / ext4 defaults 0 1\n' > "$fstab"
FAIL_DEVICE=/dev/sdb2 assert_fails_unchanged "$fstab" 'missing PARTUUID'
BOOT_MISMATCH=1 assert_fails_unchanged "$fstab" 'boot mount mismatch'
DUPLICATE_ID=1 assert_fails_unchanged "$fstab" 'duplicate PARTUUID'
WRONG_ID_TARGET=1 assert_fails_unchanged "$fstab" 'mismatched PARTUUID'
ROOT_NUMBER=4 assert_fails_unchanged "$fstab" 'unknown layout'

printf 'PARTUUID=boot-id /boot vfat defaults 0 2\n/dev/sda3 / ext4 defaults 0 1\n' > "$fstab"
IDENTITY_FAIL=1 "$NORMALIZER" "$fstab"

printf 'normalize-fstab tests passed\n'
