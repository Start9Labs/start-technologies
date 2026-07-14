#!/bin/bash
# Assemble the 0.3.5.1 -> 0.4.0 migration OTA payload.
#
# StartOS 0.3.5.1's (unchangeable) updater rsyncs the *contents* of the published
# OTA squashfs into the box's `next` subvolume, then reboots. We base the payload
# on the real 0.3.5.1 rootfs (extracted from its published release image) so the
# box's existing `next`/`current` make the rsync a small delta, and the apply-time
# `chroot next update-grub2` runs in a complete userland. On top we layer the
# 0.4.0 upgrade: the 0.4.0 base image as a nested squashfs (what the 0.4.0
# initramfs boots), the 0.4.0 kernel/initramfs, a migration sentinel, and our
# deterministic bootloader updater. The old Haskell registry loop-mounts the
# result and rsync-serves its tree unchanged.
set -eo pipefail

SOURCE_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

ARCH=
NEW_SQUASHFS=
OLD_IMAGE=
OUT=

usage() {
    >&2 echo "usage: $0 --arch ARCH --new-squashfs 0.4.0.squashfs --old-image 0.3.5.1.iso --out payload.squashfs"
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --arch)         ARCH="$2"; shift 2 ;;
        --new-squashfs) NEW_SQUASHFS="$2"; shift 2 ;;
        --old-image)    OLD_IMAGE="$2"; shift 2 ;;
        --out)          OUT="$2"; shift 2 ;;
        *) usage ;;
    esac
done
[ -n "$ARCH" ] && [ -f "$NEW_SQUASHFS" ] && [ -f "$OLD_IMAGE" ] && [ -n "$OUT" ] || usage

# Must run as root so unsquashfs/mksquashfs can restore the rootfs's ownership and
# device nodes (as non-root, unsquashfs can't and exits non-zero). Callers wrap
# this in a container rather than using host sudo (see build.mk / CI).
if [ "$(id -u)" -ne 0 ]; then
    >&2 echo "assemble-migration-payload: must run as root — wrap it in a container (the make target and CI do)"
    exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1. Extract the 0.3.5.1 base rootfs (the payload's base) from its release image.
case "$OLD_IMAGE" in
    *.iso)
        xorriso -osirrox on -indev "$OLD_IMAGE" -extract /live/filesystem.squashfs "$WORK/old.squashfs"
        ;;
    *)
        >&2 echo "assemble-migration-payload: unsupported base image $OLD_IMAGE (only .iso wired so far)"
        exit 1
        ;;
esac
unsquashfs -d "$WORK/payload" "$WORK/old.squashfs"
rm -f "$WORK/old.squashfs"

# 2. Nest the 0.4.0 base image as images/<b3sum>.rootfs — the rootfs the 0.4.0
#    initramfs installs and boots (naming matches the fresh-install layout).
B3SUM="$(b3sum "$NEW_SQUASHFS" | head -c 16)"
mkdir -p "$WORK/payload/images"
cp "$NEW_SQUASHFS" "$WORK/payload/images/$B3SUM.rootfs"

# 3. Stage the 0.4.0 kernel/initramfs; 0.3.5.1's sync_boot rsyncs boot/ -> /boot.
#    Record the exact names so update-grub2 boots the 0.4.0 kernel, not whichever
#    version happens to sort highest once /boot holds both.
rm -rf "$WORK/payload/boot"
unsquashfs -n -f -d "$WORK/payload" "$NEW_SQUASHFS" boot
mkdir -p "$WORK/payload/usr/lib/startos"
printf '%s\n%s\n' \
    "$(cd "$WORK/payload/boot" && ls -1 vmlinuz-* | head -n1)" \
    "$(cd "$WORK/payload/boot" && ls -1 initrd.img-* | head -n1)" \
    > "$WORK/payload/usr/lib/startos/migration-boot"

# 4. Sentinel the 0.4.0 initramfs keys on, plus our bootloader updater (0.3.5.1
#    execs /usr/sbin/update-grub2 in the payload chroot at apply time).
touch "$WORK/payload/.startos-migration"
install -m0755 "$SOURCE_DIR/lib/scripts/migration-update-grub" "$WORK/payload/usr/sbin/update-grub2"

# 5. Re-squash into the OTA payload the registry loop-mounts and serves.
rm -f "$OUT"
mksquashfs "$WORK/payload" "$OUT" -noappend -comp gzip -b 4096
# hand the container-created output back to the invoking user (OWNER_* passed in)
if [ -n "${OWNER_UID:-}" ]; then chown "$OWNER_UID:${OWNER_GID:-$OWNER_UID}" "$OUT"; fi

echo "migration payload for $ARCH -> $OUT (base image $B3SUM.rootfs)"
