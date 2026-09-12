#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname -- "${BASH_SOURCE[0]}")/..")
SCRIPT="$ROOT/lib/scripts/normalize-fstab"
CHROOT_SCRIPT="$ROOT/lib/scripts/chroot-and-upgrade"
MIGRATION_SCRIPT="$ROOT/lib/scripts/migration-update-grub"
TEST_DIR=$(mktemp -d)
trap 'rm -rf -- "$TEST_DIR"' EXIT

COMMAND_LOG="$TEST_DIR/commands.log"
BLKID_LOG="$TEST_DIR/blkid.log"

cat > "$TEST_DIR/findmnt" <<'EOF'
#!/bin/bash
set -euo pipefail
[ "$#" -eq 5 ] && [ "$1" = -n ] && [ "$2" = -o ] && [ "$3" = SOURCE ] && [ "$4" = --target ] || exit 64
case "${MOUNT_LAYOUT}:$5" in
    current:/media/startos/root) echo /dev/nvme0n1p3 ;;
    current:/boot) echo /dev/nvme0n1p2 ;;
    current:/boot/efi) echo /dev/nvme0n1p1 ;;
    legacy:/boot) echo /dev/vda2 ;;
    legacy-mbr:/boot) echo /dev/vda1 ;;
    wrong:/media/startos/root) echo /dev/sda3 ;;
    *) exit 1 ;;
esac
EOF

cat > "$TEST_DIR/lsblk" <<'EOF'
#!/bin/bash
set -euo pipefail
case "$*" in
    '-nro PKNAME /dev/vda1'|'-nro PKNAME /dev/vda2') echo vda ;;
    '-nro PARTN /dev/vda1') echo 1 ;;
    '-nro PARTN /dev/vda2') echo 2 ;;
    '-nrpo PATH,PARTN /dev/vda')
        printf '%s\n' '/dev/vda ' '/dev/vda1 1' '/dev/vda2 2' '/dev/vda3 3'
        ;;
    *) exit 64 ;;
esac
EOF

cat > "$TEST_DIR/blkid" <<'EOF'
#!/bin/bash
set -euo pipefail
[ "$#" -eq 6 ] && [ "$1" = -p ] && [ "$2" = -s ] && [ "$4" = -o ] && [ "$5" = value ] || exit 64
printf '%s|%s\n' "$3" "$6" >> "$BLKID_LOG"
case "$3:$6" in
    LABEL:/dev/nvme0n1p1) echo efi ;;
    LABEL:/dev/nvme0n1p2) echo boot ;;
    LABEL:/dev/nvme0n1p3) echo rootfs ;;
    PART_ENTRY_UUID:/dev/nvme0n1p1) echo current-efi ;;
    PART_ENTRY_UUID:/dev/nvme0n1p2) echo current-boot ;;
    PART_ENTRY_UUID:/dev/nvme0n1p3) echo current-root ;;
    LABEL:/dev/vda1) [ "$MOUNT_LAYOUT" = legacy-mbr ] && echo boot || echo efi ;;
    LABEL:/dev/vda2) [ "$MOUNT_LAYOUT" = legacy-mbr ] && echo rootfs || echo boot ;;
    LABEL:/dev/vda3) echo rootfs ;;
    PART_ENTRY_UUID:/dev/vda1) [ "$MOUNT_LAYOUT" = legacy-mbr ] && echo mbr-boot || echo legacy-efi ;;
    PART_ENTRY_UUID:/dev/vda2) [ "$MOUNT_LAYOUT" = legacy-mbr ] && echo mbr-root || echo legacy-boot ;;
    PART_ENTRY_UUID:/dev/vda3) echo legacy-root ;;
    LABEL:/dev/sda*) echo wrong-disk ;;
    PART_ENTRY_UUID:/dev/sda*) echo stale-partuuid ;;
    *) exit 2 ;;
esac
EOF

cat > "$TEST_DIR/sync" <<'EOF'
#!/bin/bash
set -euo pipefail
[ "$1" = -f ] && [ "$#" -eq 2 ] || exit 64
if [[ $2 = */.fstab-durable.* ]]; then
    [ "$(stat -c '%a' "$2")" = 604 ]
    grep -Fx 'PARTUUID=current-root / ext4 defaults 0 1' "$2" >/dev/null
fi
printf 'sync %s\n' "$2" >> "$COMMAND_LOG"
EOF

cat > "$TEST_DIR/mv" <<'EOF'
#!/bin/bash
set -euo pipefail
printf 'mv %s\n' "${@: -1}" >> "$COMMAND_LOG"
exec /bin/mv "$@"
EOF
chmod +x "$TEST_DIR/findmnt" "$TEST_DIR/lsblk" "$TEST_DIR/blkid" "$TEST_DIR/sync" "$TEST_DIR/mv"

run_normalizer() {
    env \
        BLKID="$TEST_DIR/blkid" BLKID_LOG="$BLKID_LOG" \
        FINDMNT="$TEST_DIR/findmnt" LSBLK="$TEST_DIR/lsblk" \
        SYNC="$TEST_DIR/sync" MV="$TEST_DIR/mv" COMMAND_LOG="$COMMAND_LOG" \
        MOUNT_LAYOUT="$MOUNT_LAYOUT" \
        "$SCRIPT" "$@"
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
/dev/sda2 /boot vfat defaults 0 2
  /dev/sda3   /   ext4   defaults,noatime   0 1 # stale path
	/dev/sda1	/boot/efi	vfat	umask=0077	0	1
/dev/mapper/boot-assets /boot/assets ext4 defaults 0 2
/dev/mapper/data /srv/data btrfs subvol=@data,compress=zstd 0 2
EOF
cat > "$expected" <<'EOF'
# root and data

UUID=unchanged-uuid /old ext4 defaults 0 2
PARTUUID=current-boot /boot vfat defaults 0 2
  PARTUUID=current-root   /   ext4   defaults,noatime   0 1 # stale path
	PARTUUID=current-efi	/boot/efi	vfat	umask=0077	0	1
/dev/mapper/boot-assets /boot/assets ext4 defaults 0 2
/dev/mapper/data /srv/data btrfs subvol=@data,compress=zstd 0 2
EOF
chmod 0640 "$fstab"
owner_before=$(stat -c '%u:%g' "$fstab")
MOUNT_LAYOUT=current run_normalizer "$fstab"
assert_files_equal "$expected" "$fstab"
[ "$(stat -c '%a' "$fstab")" = 640 ]
[ "$(stat -c '%u:%g' "$fstab")" = "$owner_before" ]
if grep -F '/dev/sda' "$BLKID_LOG" >/dev/null; then
    >&2 echo 'Read stale device identity'
    exit 1
fi
grep -Fx 'PART_ENTRY_UUID|/dev/nvme0n1p3' "$BLKID_LOG" >/dev/null
grep -Fx 'PART_ENTRY_UUID|/dev/nvme0n1p2' "$BLKID_LOG" >/dev/null
grep -Fx 'PART_ENTRY_UUID|/dev/nvme0n1p1' "$BLKID_LOG" >/dev/null

echo 'PASS live mounts override valid stale device paths'

inode_before=$(stat -c '%i' "$fstab")
MOUNT_LAYOUT=current run_normalizer "$fstab"
[ "$(stat -c '%i' "$fstab")" = "$inode_before" ]

echo 'PASS idempotence'

legacy="$TEST_DIR/fstab-legacy"
legacy_expected="$TEST_DIR/expected-legacy"
printf '/dev/sdb3 / ext4 defaults 0 1\n/dev/sdb2 /boot vfat defaults 0 2\n/dev/sdb1 /boot/efi vfat defaults 0 1' > "$legacy"
printf 'PARTUUID=legacy-root / ext4 defaults 0 1\nPARTUUID=legacy-boot /boot vfat defaults 0 2\nPARTUUID=legacy-efi /boot/efi vfat defaults 0 1' > "$legacy_expected"
MOUNT_LAYOUT=legacy run_normalizer --legacy "$legacy"
assert_files_equal "$legacy_expected" "$legacy"
[ "$(tail -c 1 "$legacy" | od -An -t x1 | tr -d '[:space:]')" != 0a ]
if grep -F '/dev/sdb' "$BLKID_LOG" >/dev/null; then
    >&2 echo 'Read stale legacy device identity'
    exit 1
fi

echo 'PASS legacy GPT boot mount selects same-disk installer partitions'

legacy_mbr="$TEST_DIR/fstab-legacy-mbr"
printf '/dev/sdb2 / ext4 defaults 0 1\n/dev/sdb1 /boot vfat defaults 0 2\n' > "$legacy_mbr"
printf 'PARTUUID=mbr-root / ext4 defaults 0 1\nPARTUUID=mbr-boot /boot vfat defaults 0 2\n' > "$legacy_mbr.expected"
MOUNT_LAYOUT=legacy-mbr run_normalizer --legacy "$legacy_mbr"
assert_files_equal "$legacy_mbr.expected" "$legacy_mbr"

echo 'PASS legacy MBR boot mount selects the following root partition'

unresolved="$TEST_DIR/fstab-unresolved"
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$unresolved"
cp "$unresolved" "$unresolved.expected"
if MOUNT_LAYOUT=missing run_normalizer "$unresolved" >"$TEST_DIR/unresolved.out" 2>"$TEST_DIR/unresolved.err"; then
    >&2 echo 'Expected missing installed identity to fail'
    exit 1
fi
assert_files_equal "$unresolved.expected" "$unresolved"
grep -F 'Unable to establish installed partition for /' "$TEST_DIR/unresolved.err" >/dev/null

echo 'PASS unresolved identity fails without replacing fstab'

wrong="$TEST_DIR/fstab-wrong"
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$wrong"
cp "$wrong" "$wrong.expected"
if MOUNT_LAYOUT=wrong run_normalizer "$wrong" >"$TEST_DIR/wrong.out" 2>"$TEST_DIR/wrong.err"; then
    >&2 echo 'Expected non-StartOS mounted partition to fail'
    exit 1
fi
assert_files_equal "$wrong.expected" "$wrong"
grep -F 'Unable to establish installed partition for /' "$TEST_DIR/wrong.err" >/dev/null

echo 'PASS mounted partition must match the StartOS layout label'

: > "$COMMAND_LOG"
durable="$TEST_DIR/fstab-durable"
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$durable"
chmod 0604 "$durable"
MOUNT_LAYOUT=current run_normalizer "$durable"
mapfile -t operations < "$COMMAND_LOG"
[ "${#operations[@]}" -eq 3 ]
[[ ${operations[0]} = "sync $TEST_DIR/.fstab-durable."* ]]
[ "${operations[1]}" = "mv $durable" ]
[ "${operations[2]}" = "sync $TEST_DIR" ]

echo 'PASS completed temp file and parent filesystem are flushed around rename'

normalize_line=$(grep -nF '/media/startos/next/usr/lib/startos/scripts/normalize-fstab /media/startos/config/overlay/etc/fstab' "$CHROOT_SCRIPT" | cut -d: -f1)
mksquashfs_line=$(grep -nF 'mksquashfs /media/startos/next' "$CHROOT_SCRIPT" | cut -d: -f1)
[ "$(grep -cF '/media/startos/next/usr/lib/startos/scripts/normalize-fstab' "$CHROOT_SCRIPT")" -eq 1 ]
[ "$normalize_line" -lt "$mksquashfs_line" ]
grep -F '/usr/lib/startos/scripts/normalize-fstab --legacy /etc/fstab' "$MIGRATION_SCRIPT" >/dev/null

echo 'PASS shared chroot activation and legacy migration invoke normalization'
