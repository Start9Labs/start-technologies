#!/bin/bash

set -euo pipefail

ROOT=$(realpath "$(dirname -- "${BASH_SOURCE[0]}")/..")
SCRIPT="$ROOT/lib/scripts/normalize-fstab"
CHROOT_SCRIPT="$ROOT/lib/scripts/chroot-and-upgrade"
ASSEMBLE_SCRIPT="$ROOT/assemble-migration-payload.sh"
TEST_DIR=$(mktemp -d)
trap 'rm -rf -- "$TEST_DIR"' EXIT

MOCK_BIN="$TEST_DIR/bin"
mkdir "$MOCK_BIN"
COMMAND_LOG="$TEST_DIR/commands.log"
BLKID_LOG="$TEST_DIR/blkid.log"
: > "$COMMAND_LOG"
: > "$BLKID_LOG"

cat > "$MOCK_BIN/findmnt" <<'EOF'
#!/bin/bash
set -euo pipefail
[ "$#" -eq 5 ] && [ "$1" = -n ] && [ "$2" = -o ] && [ "$3" = SOURCE ] && [ "$4" = --target ] || exit 64
case "${MOUNT_LAYOUT}:$5" in
    current:/media/startos/root|current-foreign-*:/media/startos/root) echo /dev/nvme0n1p3 ;;
    current:/boot|current-foreign-efi:/boot) echo /dev/nvme0n1p2 ;;
    current:/boot/efi|current-foreign-boot:/boot/efi) echo /dev/nvme0n1p1 ;;
    current-foreign-boot:/boot) echo /dev/sdb2 ;;
    current-foreign-efi:/boot/efi) echo /dev/sdb1 ;;
    legacy:/) echo /dev/vda3 ;;
    legacy:/boot|legacy:/boot/efi) echo /dev/vda2 ;;
    legacy-mbr:/) echo /dev/vda2 ;;
    legacy-mbr:/boot) echo /dev/vda1 ;;
    wrong:/media/startos/root) echo /dev/sda3 ;;
    *) exit 1 ;;
esac
EOF

cat > "$MOCK_BIN/lsblk" <<'EOF'
#!/bin/bash
set -euo pipefail
case "$*" in
    '-nro PKNAME /dev/nvme0n1p3') echo nvme0n1 ;;
    '-nro PARTN /dev/nvme0n1p3') echo 3 ;;
    '-nrpo PATH,PARTN /dev/nvme0n1')
        printf '%s\n' '/dev/nvme0n1 ' '/dev/nvme0n1p1 1' '/dev/nvme0n1p2 2' '/dev/nvme0n1p3 3'
        ;;
    '-nro PKNAME /dev/vda2'|'-nro PKNAME /dev/vda3') echo vda ;;
    '-nro PARTN /dev/vda2') echo 2 ;;
    '-nro PARTN /dev/vda3') echo 3 ;;
    '-nrpo PATH,PARTN /dev/vda')
        printf '%s\n' '/dev/vda ' '/dev/vda1 1' '/dev/vda2 2' '/dev/vda3 3'
        ;;
    '-nro PKNAME /dev/sda3') echo sda ;;
    '-nro PARTN /dev/sda3') echo 3 ;;
    '-nrpo PATH,PARTN /dev/sda')
        printf '%s\n' '/dev/sda ' '/dev/sda1 1' '/dev/sda2 2' '/dev/sda3 3'
        ;;
    *) exit 64 ;;
esac
EOF

cat > "$MOCK_BIN/blkid" <<'EOF'
#!/bin/bash
set -euo pipefail
if [ "$#" -eq 6 ] && [ "$1" = -p ] && [ "$2" = -s ] && [ "$4" = -o ] && [ "$5" = value ]; then
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
        LABEL:/dev/sda1) echo efi ;;
        LABEL:/dev/sda2) echo boot ;;
        LABEL:/dev/sda3) [ "$MOUNT_LAYOUT" = wrong ] && echo wrong-disk || echo rootfs ;;
        PART_ENTRY_UUID:/dev/sda*) echo stale-partuuid ;;
        LABEL:/dev/sdb1) echo efi ;;
        LABEL:/dev/sdb2) echo boot ;;
        LABEL:/dev/sdb3) echo rootfs ;;
        PART_ENTRY_UUID:/dev/sdb1) echo foreign-efi ;;
        PART_ENTRY_UUID:/dev/sdb2) echo foreign-boot ;;
        PART_ENTRY_UUID:/dev/sdb3) echo foreign-root ;;
        *) exit 2 ;;
    esac
elif [ "$#" -eq 4 ] && [ "$1" = -t ] && [ "$3" = -o ] && [ "$4" = device ]; then
    partuuid=${2#PARTUUID=}
    printf 'LOOKUP|%s\n' "$partuuid" >> "$BLKID_LOG"
    case "${GLOBAL_PARTUUID_LAYOUT:-match}:$partuuid" in
        match:current-efi) echo /dev/nvme0n1p1 ;;
        match:current-boot) echo /dev/nvme0n1p2 ;;
        match:current-root) echo /dev/nvme0n1p3 ;;
        match:legacy-efi) echo /dev/vda1 ;;
        match:legacy-boot) echo /dev/vda2 ;;
        match:legacy-root) echo /dev/vda3 ;;
        match:mbr-boot) echo /dev/vda1 ;;
        match:mbr-root) echo /dev/vda2 ;;
        duplicate:current-root) printf '%s\n' /dev/nvme0n1p3 /dev/sda3 ;;
        mismatch:current-root) echo /dev/sda3 ;;
        none:current-root) exit 2 ;;
        *) exit 2 ;;
    esac
else
    exit 64
fi
EOF

cat > "$MOCK_BIN/sync" <<'EOF'
#!/bin/bash
set -euo pipefail
[ "$#" -le 1 ] || exit 64
if [ "$#" -eq 1 ]; then
    if [[ $1 = */.fstab-durable.* ]]; then
        [ "$(stat -c '%a' "$1")" = 604 ]
        grep -Fx 'PARTUUID=current-root / ext4 defaults 0 1' "$1" >/dev/null
    fi
    printf 'sync %s\n' "$1" >> "$COMMAND_LOG"
else
    printf 'sync\n' >> "$COMMAND_LOG"
fi
EOF

cat > "$MOCK_BIN/mv" <<'EOF'
#!/bin/bash
set -euo pipefail
printf 'mv %s\n' "${@: -1}" >> "$COMMAND_LOG"
exec /bin/mv "$@"
EOF

cat > "$MOCK_BIN/cp" <<'EOF'
#!/bin/bash
set -euo pipefail
if [ "${FAIL_SOURCE_COPY:-0}" -eq 1 ]; then
    args=("$@")
    source=${args[$((${#args[@]} - 2))]}
    destination=${args[$((${#args[@]} - 1))]}
    /bin/dd if="$source" of="$destination" bs=1 count=8 status=none
    exit 74
fi
exec /bin/cp "$@"
EOF

cat > "$MOCK_BIN/cmp" <<'EOF'
#!/bin/bash
set -euo pipefail
if [ -n "${CMP_FAIL_AT:-}" ]; then
    count=$(cat "$CMP_COUNT")
    count=$((count + 1))
    printf '%s\n' "$count" > "$CMP_COUNT"
    [ "$count" -ne "$CMP_FAIL_AT" ] || exit 2
fi
exec /usr/bin/cmp "$@"
EOF
chmod +x "$MOCK_BIN"/*

run_normalizer() {
    env \
        PATH="$MOCK_BIN:$PATH" BLKID_LOG="$BLKID_LOG" \
        COMMAND_LOG="$COMMAND_LOG" MOUNT_LAYOUT="$MOUNT_LAYOUT" \
        GLOBAL_PARTUUID_LAYOUT="${GLOBAL_PARTUUID_LAYOUT:-match}" "$SCRIPT" "$@"
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
touch -d '@946684800' "$fstab"
owner_before=$(stat -c '%u:%g' "$fstab")
mtime_before=$(stat -c '%Y' "$fstab")
MOUNT_LAYOUT=current run_normalizer "$fstab"
assert_files_equal "$expected" "$fstab"
[ "$(stat -c '%a' "$fstab")" = 640 ]
[ "$(stat -c '%u:%g' "$fstab")" = "$owner_before" ]
[ "$(stat -c '%Y' "$fstab")" = "$mtime_before" ]
if grep -F '/dev/sda' "$BLKID_LOG" >/dev/null; then
    >&2 echo 'Read stale device identity'
    exit 1
fi
grep -Fx 'PART_ENTRY_UUID|/dev/nvme0n1p3' "$BLKID_LOG" >/dev/null
grep -Fx 'PART_ENTRY_UUID|/dev/nvme0n1p2' "$BLKID_LOG" >/dev/null
grep -Fx 'PART_ENTRY_UUID|/dev/nvme0n1p1' "$BLKID_LOG" >/dev/null

echo 'PASS live mounts override valid stale device paths'

bios_fstab="$TEST_DIR/fstab-bios"
printf '/dev/sda3 / ext4 defaults 0 1\n/dev/sda2 /boot vfat defaults 0 2\n' > "$bios_fstab"
printf 'PARTUUID=current-root / ext4 defaults 0 1\nPARTUUID=current-boot /boot vfat defaults 0 2\n' > "$bios_fstab.expected"
MOUNT_LAYOUT=current run_normalizer "$bios_fstab"
assert_files_equal "$bios_fstab.expected" "$bios_fstab"

echo 'PASS BIOS GPT layout does not require an EFI mount'

inode_before=$(stat -c '%i' "$fstab")
MOUNT_LAYOUT=current run_normalizer "$fstab"
[ "$(stat -c '%i' "$fstab")" = "$inode_before" ]

echo 'PASS idempotence'

symlink_dir="$TEST_DIR/fstab-symlink"
mkdir "$symlink_dir"
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$symlink_dir/target"
ln -s target "$symlink_dir/fstab"
MOUNT_LAYOUT=current run_normalizer "$symlink_dir/fstab"
[ -L "$symlink_dir/fstab" ]
[ "$(readlink "$symlink_dir/fstab")" = target ]
printf 'PARTUUID=current-root / ext4 defaults 0 1\n' > "$symlink_dir/expected"
assert_files_equal "$symlink_dir/expected" "$symlink_dir/target"

echo 'PASS symlink destination remains linked and its target is normalized'

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
/bin/cp "$unresolved" "$unresolved.expected"
if MOUNT_LAYOUT=missing run_normalizer "$unresolved" >"$TEST_DIR/unresolved.out" 2>"$TEST_DIR/unresolved.err"; then
    >&2 echo 'Expected missing installed identity to fail'
    exit 1
fi
assert_files_equal "$unresolved.expected" "$unresolved"
grep -F 'Unable to establish installed partition for /' "$TEST_DIR/unresolved.err" >/dev/null

echo 'PASS unresolved identity fails without replacing fstab'

wrong="$TEST_DIR/fstab-wrong"
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$wrong"
/bin/cp "$wrong" "$wrong.expected"
if MOUNT_LAYOUT=wrong run_normalizer "$wrong" >"$TEST_DIR/wrong.out" 2>"$TEST_DIR/wrong.err"; then
    >&2 echo 'Expected non-StartOS mounted partition to fail'
    exit 1
fi
assert_files_equal "$wrong.expected" "$wrong"
grep -F 'Unable to establish installed partition for /' "$TEST_DIR/wrong.err" >/dev/null

echo 'PASS mounted partition must match the StartOS layout label'

for global_layout in duplicate mismatch none; do
    global_failure="$TEST_DIR/fstab-global-$global_layout"
    printf '/dev/sda3 / ext4 defaults 0 1\n' > "$global_failure"
    /bin/cp "$global_failure" "$global_failure.expected"
    if GLOBAL_PARTUUID_LAYOUT="$global_layout" MOUNT_LAYOUT=current run_normalizer "$global_failure" \
        >"$TEST_DIR/global-$global_layout.out" 2>"$TEST_DIR/global-$global_layout.err"; then
        >&2 echo "Expected $global_layout global PARTUUID lookup to fail"
        exit 1
    fi
    assert_files_equal "$global_failure.expected" "$global_failure"
done
grep -F 'Unable to uniquely resolve PARTUUID for installed partition /' "$TEST_DIR/global-duplicate.err" >/dev/null
grep -F 'resolves to another device' "$TEST_DIR/global-mismatch.err" >/dev/null
grep -F 'Unable to uniquely resolve PARTUUID for installed partition /' "$TEST_DIR/global-none.err" >/dev/null

echo 'PASS PARTUUID must resolve globally and uniquely to the installed partition'

for target in boot efi; do
    foreign_boot="$TEST_DIR/fstab-foreign-$target"
    printf '/dev/sda3 / ext4 defaults 0 1\n/dev/sda2 /boot vfat defaults 0 2\n/dev/sda1 /boot/efi vfat defaults 0 1\n' > "$foreign_boot"
    /bin/cp "$foreign_boot" "$foreign_boot.expected"
    if MOUNT_LAYOUT="current-foreign-$target" run_normalizer "$foreign_boot" \
        >"$TEST_DIR/foreign-$target.out" 2>"$TEST_DIR/foreign-$target.err"; then
        >&2 echo "Expected another StartOS disk mounted at $target to fail"
        exit 1
    fi
    assert_files_equal "$foreign_boot.expected" "$foreign_boot"
done
grep -F 'Unable to establish installed partition for /boot' "$TEST_DIR/foreign-boot.err" >/dev/null
grep -F 'Unable to establish installed partition for /boot/efi' "$TEST_DIR/foreign-efi.err" >/dev/null

echo 'PASS boot and EFI mounts from another correctly-labelled StartOS disk are rejected'

read_failure="$TEST_DIR/fstab-read-failure"
printf '/dev/sda3 / ext4 defaults 0 1\nsecond line that must survive\n' > "$read_failure"
/bin/cp "$read_failure" "$read_failure.expected"
: > "$COMMAND_LOG"
if env PATH="$MOCK_BIN:$PATH" BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" \
    MOUNT_LAYOUT=current FAIL_SOURCE_COPY=1 "$SCRIPT" "$read_failure"; then
    >&2 echo 'Expected partial source copy to fail'
    exit 1
fi
assert_files_equal "$read_failure.expected" "$read_failure"
if grep -F "mv $read_failure" "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Partial source copy reached rename'
    exit 1
fi

echo 'PASS failed complete source read cannot replace fstab'

for cmp_fail_at in 1 2; do
    compare_failure="$TEST_DIR/fstab-compare-failure-$cmp_fail_at"
    printf '/dev/sda3 / ext4 defaults 0 1\n' > "$compare_failure"
    /bin/cp "$compare_failure" "$compare_failure.expected"
    : > "$COMMAND_LOG"
    printf '0\n' > "$TEST_DIR/cmp-count"
    if env PATH="$MOCK_BIN:$PATH" BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" \
        MOUNT_LAYOUT=current CMP_FAIL_AT="$cmp_fail_at" CMP_COUNT="$TEST_DIR/cmp-count" \
        "$SCRIPT" "$compare_failure"; then
        >&2 echo "Expected comparison $cmp_fail_at to fail"
        exit 1
    fi
    assert_files_equal "$compare_failure.expected" "$compare_failure"
    if grep -F "mv $compare_failure" "$COMMAND_LOG" >/dev/null; then
        >&2 echo "Comparison $cmp_fail_at failure reached rename"
        exit 1
    fi
done

echo 'PASS failed source comparisons cannot replace fstab'

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

echo 'PASS completed temp file and parent directory are flushed around rename'

INTEGRATION_BIN="$TEST_DIR/integration-bin"
mkdir "$INTEGRATION_BIN"
for command in blkid cmp cp findmnt lsblk mv sync; do
    ln -s "$MOCK_BIN/$command" "$INTEGRATION_BIN/$command"
done
cat > "$INTEGRATION_BIN/id" <<'EOF'
#!/bin/bash
[ "$1" = -u ] && echo 0
EOF
cat > "$INTEGRATION_BIN/mountpoint" <<'EOF'
#!/bin/bash
case "${@: -1}" in
    */media/startos/next|*/media/startos/upper)
        [ "${OUTER_MOUNTS_READY:-0}" -eq 1 ]
        ;;
    *) exit 1 ;;
esac
EOF
cat > "$INTEGRATION_BIN/mount" <<'EOF'
#!/bin/bash
set -euo pipefail
printf 'mount %s\n' "$*" >> "$COMMAND_LOG"
if [ -n "${FAIL_MOUNT_AT:-}" ]; then
    count=$(cat "$MOUNT_COUNT")
    count=$((count + 1))
    printf '%s\n' "$count" > "$MOUNT_COUNT"
    [ "$count" -ne "$FAIL_MOUNT_AT" ] || exit 32
fi
EOF
cat > "$INTEGRATION_BIN/umount" <<'EOF'
#!/bin/bash
set -euo pipefail
printf 'umount %s\n' "$*" >> "$COMMAND_LOG"
if [ -n "${FAIL_UMOUNT_AT:-}" ]; then
    count=$(cat "$UMOUNT_COUNT")
    count=$((count + 1))
    printf '%s\n' "$count" > "$UMOUNT_COUNT"
    [ "$count" -ne "$FAIL_UMOUNT_AT" ] || exit 32
fi
EOF
cat > "$INTEGRATION_BIN/chroot" <<'EOF'
#!/bin/bash
set -euo pipefail
printf 'chroot %s\n' "$*" >> "$COMMAND_LOG"
if [ "${REMOVE_TARGET_HELPER:-0}" -eq 1 ]; then
    rm -f "$1/usr/lib/startos/scripts/normalize-fstab"
    printf 'remove-target-helper %s\n' "$1/usr/lib/startos/scripts/normalize-fstab" >> "$COMMAND_LOG"
fi
if [ -n "${CHROOT_SIGNAL:-}" ]; then
    kill "-$CHROOT_SIGNAL" "$PPID"
fi
exit "${CHROOT_EXIT:-0}"
EOF
cat > "$INTEGRATION_BIN/mksquashfs" <<'EOF'
#!/bin/bash
set -euo pipefail
printf 'mksquashfs %s\n' "$*" >> "$COMMAND_LOG"
mkdir -p "$(dirname "$2")"
: > "$2"
if [ -n "${PAYLOAD_CAPTURE:-}" ]; then
    rm -rf "$PAYLOAD_CAPTURE"
    /bin/cp -a "$1" "$PAYLOAD_CAPTURE"
fi
EOF
cat > "$INTEGRATION_BIN/b3sum" <<'EOF'
#!/bin/bash
printf '0123456789abcdef0123456789abcdef  %s\n' "$1"
EOF
cat > "$INTEGRATION_BIN/reboot" <<'EOF'
#!/bin/bash
printf 'reboot\n' >> "$COMMAND_LOG"
EOF
cat > "$INTEGRATION_BIN/xorriso" <<'EOF'
#!/bin/bash
set -euo pipefail
: > "${@: -1}"
EOF
cat > "$INTEGRATION_BIN/unsquashfs" <<'EOF'
#!/bin/bash
set -euo pipefail
for ((i = 1; i <= $#; i++)); do
    if [ "${!i}" = -d ]; then
        j=$((i + 1))
        destination=${!j}
        break
    fi
done
mkdir -p "$destination/etc" "$destination/boot" "$destination/usr/lib/startos/scripts" "$destination/usr/sbin"
if [ "${@: -1}" = boot ]; then
    : > "$destination/boot/vmlinuz-test"
    : > "$destination/boot/initrd.img-test"
else
    printf '/dev/sdb3 / ext4 defaults 0 1\n/dev/sdb2 /boot vfat defaults 0 2\n/dev/sdb1 /boot/efi vfat defaults 0 1\n' > "$destination/etc/fstab"
fi
EOF
chmod +x "$INTEGRATION_BIN"/*

ota_root="$TEST_DIR/ota-root"
ota_scripts="$ota_root/usr/lib/startos/scripts"
mkdir -p "$ota_scripts" "$ota_root/media/startos/config/overlay/etc" "$ota_root/media/startos/images" "$ota_root/media/startos/root"
/bin/cp "$CHROOT_SCRIPT" "$ota_scripts/chroot-and-upgrade"
/bin/cp "$SCRIPT" "$ota_scripts/normalize-fstab"
chmod +x "$ota_scripts"/*
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$ota_root/media/startos/config/overlay/etc/fstab"

: > "$COMMAND_LOG"
if env PATH="$INTEGRATION_BIN:/usr/bin:/bin" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA=relative/path \
    "$ota_scripts/chroot-and-upgrade" true >"$TEST_DIR/relative-media.out" 2>"$TEST_DIR/relative-media.err"; then
    >&2 echo 'Expected relative STARTOS_MEDIA override to fail'
    exit 1
fi
grep -F 'STARTOS_MEDIA must be an absolute path' "$TEST_DIR/relative-media.err" >/dev/null
[ ! -s "$COMMAND_LOG" ]

echo 'PASS STARTOS_MEDIA accepts only an explicit absolute test override'

: > "$COMMAND_LOG"
if env PATH="$INTEGRATION_BIN:/usr/bin:/bin" COMMAND_LOG="$COMMAND_LOG" \
    STARTOS_MEDIA="$ota_root/media/startos" "$ota_scripts/chroot-and-upgrade" --create --no-sync; then
    >&2 echo 'Expected --create --no-sync to fail'
    exit 1
fi
[ ! -s "$COMMAND_LOG" ]

echo 'PASS --create rejects --no-sync rather than accepting absent staging mounts'

: > "$COMMAND_LOG"
if env PATH="$INTEGRATION_BIN:/usr/bin:/bin" COMMAND_LOG="$COMMAND_LOG" \
    STARTOS_MEDIA="$ota_root/media/startos" "$ota_scripts/chroot-and-upgrade" --no-sync true; then
    >&2 echo 'Expected --no-sync without staging mounts to fail'
    exit 1
fi
if grep -E '^mount |^chroot ' "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Missing staging mounts reached chroot setup'
    exit 1
fi

echo 'PASS --no-sync requires both staging mounts'

: > "$COMMAND_LOG"
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" COMMAND_LOG="$COMMAND_LOG" \
    STARTOS_MEDIA="$ota_root/media/startos" "$ota_scripts/chroot-and-upgrade" --create
if grep '^umount ' "$COMMAND_LOG" >/dev/null; then
    >&2 echo '--create cleaned the staging mounts it must preserve'
    exit 1
fi
[ -d "$ota_root/media/startos/next" ]
[ -d "$ota_root/media/startos/upper" ]

echo 'PASS --create preserves the staging mounts for the next invocation'

: > "$COMMAND_LOG"
printf '0\n' > "$TEST_DIR/mount-count"
if env PATH="$INTEGRATION_BIN:/usr/bin:/bin" SHELL=/bin/bash MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA="$ota_root/media/startos" \
    FAIL_MOUNT_AT=5 MOUNT_COUNT="$TEST_DIR/mount-count" "$ota_scripts/chroot-and-upgrade" true; then
    >&2 echo 'Expected required mount failure'
    exit 1
fi
if grep -F 'chroot ' "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Required mount failure reached chroot'
    exit 1
fi
mapfile -t failed_mount_cleanup < <(grep '^umount ' "$COMMAND_LOG" | tail -n 4)
[ "${failed_mount_cleanup[0]}" = "umount -l $ota_root/media/startos/next/tmp" ]
[ "${failed_mount_cleanup[1]}" = "umount -l $ota_root/media/startos/next/run" ]
[ "${failed_mount_cleanup[2]}" = "umount -l $ota_root/media/startos/next" ]
[ "${failed_mount_cleanup[3]}" = "umount -l $ota_root/media/startos/upper" ]

echo 'PASS required mount failure stops before chroot and cleans successful mounts'

mkdir -p "$ota_root/media/startos/next" "$ota_root/media/startos/upper"
: > "$COMMAND_LOG"
chroot_status=0
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" SHELL=/bin/bash MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA="$ota_root/media/startos" \
    OUTER_MOUNTS_READY=1 CHROOT_EXIT=42 "$ota_scripts/chroot-and-upgrade" --no-sync true || chroot_status=$?
[ "$chroot_status" -eq 42 ]
if grep -F 'mksquashfs ' "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Failed chroot reached image creation'
    exit 1
fi
mapfile -t chroot_failure_cleanup < <(grep '^umount ' "$COMMAND_LOG")
expected_cleanup=(
    "umount -l $ota_root/media/startos/next/media/startos/root"
    "umount -l $ota_root/media/startos/next/boot"
    "umount -l $ota_root/media/startos/next/proc"
    "umount -l $ota_root/media/startos/next/sys"
    "umount -l $ota_root/media/startos/next/dev"
    "umount -l $ota_root/media/startos/next/tmp"
    "umount -l $ota_root/media/startos/next/run"
    "umount -l $ota_root/media/startos/next"
    "umount -l $ota_root/media/startos/upper"
)
[ "${chroot_failure_cleanup[*]}" = "${expected_cleanup[*]}" ]

echo 'PASS failed chroot cleans invocation mounts in reverse order'

mkdir -p "$ota_root/media/startos/next" "$ota_root/media/startos/upper"
: > "$COMMAND_LOG"
printf '0\n' > "$TEST_DIR/umount-count"
unmount_status=0
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" SHELL=/bin/bash MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA="$ota_root/media/startos" \
    OUTER_MOUNTS_READY=1 FAIL_UMOUNT_AT=1 UMOUNT_COUNT="$TEST_DIR/umount-count" \
    "$ota_scripts/chroot-and-upgrade" --no-sync true || unmount_status=$?
[ "$unmount_status" -eq 1 ]
if grep -F 'mksquashfs ' "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Failed inner unmount reached image creation'
    exit 1
fi
[ "$(grep -Fc "umount -l $ota_root/media/startos/next/media/startos/root" "$COMMAND_LOG")" -eq 2 ]

echo 'PASS failed inner unmount stops image creation and is retried during cleanup'

mkdir -p "$ota_root/media/startos/next" "$ota_root/media/startos/upper"
: > "$COMMAND_LOG"
signal_status=0
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" SHELL=/bin/bash MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA="$ota_root/media/startos" \
    OUTER_MOUNTS_READY=1 CHROOT_SIGNAL=TERM "$ota_scripts/chroot-and-upgrade" --no-sync true \
    || signal_status=$?
[ "$signal_status" -eq 143 ]
if grep -F 'mksquashfs ' "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Terminated chroot reached image creation'
    exit 1
fi
grep -Fx "umount -l $ota_root/media/startos/next/media/startos/root" "$COMMAND_LOG" >/dev/null
grep -Fx "umount -l $ota_root/media/startos/next" "$COMMAND_LOG" >/dev/null

echo 'PASS catchable termination cleans inner and outer mounts'

mkdir -p "$ota_root/media/startos/next" "$ota_root/media/startos/upper"
: > "$COMMAND_LOG"
printf '0\n' > "$TEST_DIR/umount-count"
outer_unmount_status=0
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" SHELL=/bin/bash MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA="$ota_root/media/startos" \
    OUTER_MOUNTS_READY=1 FAIL_UMOUNT_AT=8 UMOUNT_COUNT="$TEST_DIR/umount-count" \
    "$ota_scripts/chroot-and-upgrade" --no-sync true || outer_unmount_status=$?
[ "$outer_unmount_status" -eq 1 ]
grep -F "mksquashfs $ota_root/media/startos/next" "$COMMAND_LOG" >/dev/null
if grep -E 'reboot|mv .*/images/.*\.rootfs' "$COMMAND_LOG" >/dev/null; then
    >&2 echo 'Failed outer unmount activated the staged image'
    exit 1
fi

echo 'PASS failed outer unmount prevents image activation'

printf '/dev/sda3 / ext4 defaults 0 1\n' > "$ota_root/media/startos/config/overlay/etc/fstab"
: > "$COMMAND_LOG"
env -u SHELL PATH="$INTEGRATION_BIN:/usr/bin:/bin" MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" STARTOS_MEDIA="$ota_root/media/startos" \
    "$ota_scripts/chroot-and-upgrade" true
printf 'PARTUUID=current-root / ext4 defaults 0 1\n' > "$TEST_DIR/ota.expected"
assert_files_equal "$TEST_DIR/ota.expected" "$ota_root/media/startos/config/overlay/etc/fstab"
normalize_operation=$(grep -nF "mv $ota_root/media/startos/config/overlay/etc/fstab" "$COMMAND_LOG" | cut -d: -f1)
image_operation=$(grep -nF "mksquashfs $ota_root/media/startos/next" "$COMMAND_LOG" | cut -d: -f1)
[ "$normalize_operation" -lt "$image_operation" ]
[ ! -e "$ota_root/media/startos/next/usr/lib/startos/scripts/normalize-fstab" ]

echo 'PASS installed OTA wrapper uses its stable helper before image creation'

rm -f "$ota_root/media/startos/config/current.rootfs"
staged_scripts="$ota_root/media/startos/next/usr/lib/startos/scripts"
mkdir -p "$staged_scripts"
/bin/cp "$CHROOT_SCRIPT" "$staged_scripts/chroot-and-upgrade"
/bin/cp "$SCRIPT" "$staged_scripts/normalize-fstab"
chmod +x "$staged_scripts"/*
printf '#!/bin/bash\nexit 99\n' > "$ota_scripts/normalize-fstab"
chmod +x "$ota_scripts/normalize-fstab"
printf '/dev/sda3 / ext4 defaults 0 1\n' > "$ota_root/media/startos/config/overlay/etc/fstab"
: > "$COMMAND_LOG"
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" SHELL=/bin/bash MOUNT_LAYOUT=current \
    BLKID_LOG="$BLKID_LOG" COMMAND_LOG="$COMMAND_LOG" REMOVE_TARGET_HELPER=1 \
    STARTOS_MEDIA="$ota_root/media/startos" OUTER_MOUNTS_READY=1 \
    "$staged_scripts/chroot-and-upgrade" --no-sync true
assert_files_equal "$TEST_DIR/ota.expected" "$ota_root/media/startos/config/overlay/etc/fstab"
grep -Fx "chroot $ota_root/media/startos/next /bin/bash -c true" "$COMMAND_LOG" >/dev/null
remove_operation=$(grep -nFx "remove-target-helper $staged_scripts/normalize-fstab" "$COMMAND_LOG" | cut -d: -f1)
normalize_operation=$(grep -nF "mv $ota_root/media/startos/config/overlay/etc/fstab" "$COMMAND_LOG" | cut -d: -f1)
image_operation=$(grep -nF "mksquashfs $ota_root/media/startos/next" "$COMMAND_LOG" | cut -d: -f1)
[ "$remove_operation" -lt "$normalize_operation" ]
[ "$normalize_operation" -lt "$image_operation" ]

echo 'PASS staged OTA wrapper snapshots its sibling and survives target helper removal'

old_image="$TEST_DIR/old.iso"
new_squashfs="$TEST_DIR/new.squashfs"
payload="$TEST_DIR/payload.squashfs"
payload_capture="$TEST_DIR/payload-root"
: > "$old_image"
: > "$new_squashfs"
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" COMMAND_LOG="$COMMAND_LOG" PAYLOAD_CAPTURE="$payload_capture" \
    "$ASSEMBLE_SCRIPT" --arch x86_64 --new-squashfs "$new_squashfs" \
    --old-image "$old_image" --out "$payload"
[ -x "$payload_capture/usr/sbin/update-grub2" ]
[ -x "$payload_capture/usr/lib/startos/scripts/normalize-fstab" ]
mkdir -p "$payload_capture/proc"
printf 'quiet root=UUID=legacy-root ro\n' > "$payload_capture/proc/cmdline"
env PATH="$INTEGRATION_BIN:/usr/bin:/bin" MOUNT_LAYOUT=legacy BLKID_LOG="$BLKID_LOG" \
    COMMAND_LOG="$COMMAND_LOG" "$payload_capture/usr/sbin/update-grub2"
printf 'PARTUUID=legacy-root / ext4 defaults 0 1\nPARTUUID=legacy-boot /boot vfat defaults 0 2\nPARTUUID=legacy-efi /boot/efi vfat defaults 0 1\n' > "$TEST_DIR/migration.expected"
assert_files_equal "$TEST_DIR/migration.expected" "$payload_capture/etc/fstab"
grep -F 'linux /vmlinuz-test root=UUID=legacy-root boot=startos' "$payload_capture/boot/grub/grub.cfg" >/dev/null

echo 'PASS migration payload stages and reaches legacy normalization'
