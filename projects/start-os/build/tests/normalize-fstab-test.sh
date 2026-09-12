#!/bin/bash

set -euo pipefail

SCRIPT=$(realpath "$(dirname -- "${BASH_SOURCE[0]}")/../lib/scripts/normalize-fstab")
TEST_DIR=$(mktemp -d)
trap 'rm -rf -- "$TEST_DIR"' EXIT

FAKE_BLKID="$TEST_DIR/blkid"
BLKID_LOG="$TEST_DIR/blkid.log"
cat > "$FAKE_BLKID" <<'EOF'
#!/bin/bash
set -euo pipefail

if [ "$#" -ne 6 ] || [ "$1" != -p ] || [ "$2" != -s ] ||
    [ "$3" != PART_ENTRY_UUID ] || [ "$4" != -o ] || [ "$5" != value ]; then
    exit 64
fi

printf '%s|%s|%s|%s|%s|%s\n' "$@" >> "$BLKID_LOG"
case "$6" in
    /dev/sda1) printf '%s\n' '7f3a2b1c-01' ;;
    /dev/nvme0n1p2) printf '%s\n' 'f81d4fae-7dec-11d0-a765-00a0c91e6bf6' ;;
    /dev/mmcblk0p1) printf '%s\n' 'a1b2c3d4-02' ;;
    /dev/missing) exit 2 ;;
    *) exit 3 ;;
esac
EOF
chmod +x "$FAKE_BLKID"

run_normalizer() {
    env BLKID="$FAKE_BLKID" BLKID_LOG="$BLKID_LOG" "$SCRIPT" "$@"
}

assert_files_equal() {
    if ! cmp -s -- "$1" "$2"; then
        diff -u -- "$1" "$2" >&2 || true
        exit 1
    fi
}

fstab="$TEST_DIR/fstab"
expected="$TEST_DIR/expected"
cat > "$fstab" <<'EOF'
# root and data

UUID=unchanged-uuid /old ext4 defaults 0 2
PARTUUID=unchanged-partuuid /boot vfat defaults 0 2
proc /proc proc defaults 0 0
tmpfs /run tmpfs nosuid,nodev 0 0
  /dev/sda1   /   ext4   defaults,noatime   0 1 # root
	/dev/nvme0n1p2	/boot/efi	vfat	umask=0077	0	1
/dev/mapper/data /srv/data btrfs subvol=@data,compress=zstd 0 2
EOF
cat > "$expected" <<'EOF'
# root and data

UUID=unchanged-uuid /old ext4 defaults 0 2
PARTUUID=unchanged-partuuid /boot vfat defaults 0 2
proc /proc proc defaults 0 0
tmpfs /run tmpfs nosuid,nodev 0 0
  PARTUUID=7f3a2b1c-01   /   ext4   defaults,noatime   0 1 # root
	PARTUUID=f81d4fae-7dec-11d0-a765-00a0c91e6bf6	/boot/efi	vfat	umask=0077	0	1
/dev/mapper/data /srv/data btrfs subvol=@data,compress=zstd 0 2
EOF
chmod 0640 "$fstab"
owner_before=$(stat -c '%u:%g' "$fstab")
run_normalizer "$fstab"
assert_files_equal "$expected" "$fstab"
[ "$(stat -c '%a' "$fstab")" = 640 ]
[ "$(stat -c '%u:%g' "$fstab")" = "$owner_before" ]
grep -Fx -- '-p|-s|PART_ENTRY_UUID|-o|value|/dev/sda1' "$BLKID_LOG" >/dev/null
grep -Fx -- '-p|-s|PART_ENTRY_UUID|-o|value|/dev/nvme0n1p2' "$BLKID_LOG" >/dev/null
[ "$(wc -l < "$BLKID_LOG")" -eq 2 ]

echo 'PASS conversion, field preservation, unrelated sources, mode, and ownership'

inode_before=$(stat -c '%i' "$fstab")
run_normalizer "$fstab"
assert_files_equal "$expected" "$fstab"
[ "$(stat -c '%i' "$fstab")" = "$inode_before" ]
[ "$(wc -l < "$BLKID_LOG")" -eq 2 ]

echo 'PASS idempotence'

no_newline="$TEST_DIR/fstab-no-newline"
no_newline_expected="$TEST_DIR/expected-no-newline"
printf '  /dev/mmcblk0p1\t/boot\tvfat\tdefaults\t0\t2' > "$no_newline"
printf '  PARTUUID=a1b2c3d4-02\t/boot\tvfat\tdefaults\t0\t2' > "$no_newline_expected"
run_normalizer "$no_newline"
assert_files_equal "$no_newline_expected" "$no_newline"
[ "$(tail -c 1 "$no_newline" | od -An -t x1 | tr -d '[:space:]')" != 0a ]

echo 'PASS whitespace and missing final newline'

atomic="$TEST_DIR/fstab-atomic"
atomic_expected="$TEST_DIR/expected-atomic"
cat > "$atomic" <<'EOF'
/dev/sda1 / ext4 defaults 0 1
/dev/missing /boot/efi vfat umask=0077 0 1
EOF
cp "$atomic" "$atomic_expected"
chmod 0604 "$atomic"
if run_normalizer "$atomic" >"$TEST_DIR/unresolved.out" 2>"$TEST_DIR/unresolved.err"; then
    >&2 echo 'Expected unresolved source to fail'
    exit 1
fi
assert_files_equal "$atomic_expected" "$atomic"
[ "$(stat -c '%a' "$atomic")" = 604 ]
grep -F 'Unable to resolve PARTUUID for /dev/missing' "$TEST_DIR/unresolved.err" >/dev/null

echo 'PASS unresolved source atomic failure'
