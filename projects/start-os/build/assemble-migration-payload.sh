#!/bin/bash
# The legacy updater rsyncs the entire payload before invoking update-grub2.
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

if [ "$(id -u)" -ne 0 ]; then
    >&2 echo "assemble-migration-payload: must run as root — wrap it in a container (the make target and CI do)"
    exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

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

# The initramfs installs images/<16-character b3sum>.rootfs.
B3SUM="$(b3sum "$NEW_SQUASHFS" | head -c 16)"
mkdir -p "$WORK/payload/images"
cp "$NEW_SQUASHFS" "$WORK/payload/images/$B3SUM.rootfs"

# The migration must name the staged kernel exactly.
rm -rf "$WORK/payload/boot"
unsquashfs -n -f -d "$WORK/payload" "$NEW_SQUASHFS" boot
mkdir -p "$WORK/payload/usr/lib/startos"
printf '%s\n%s\n' \
    "$(cd "$WORK/payload/boot" && ls -1 vmlinuz-* | head -n1)" \
    "$(cd "$WORK/payload/boot" && ls -1 initrd.img-* | head -n1)" \
    > "$WORK/payload/usr/lib/startos/migration-boot"

# The legacy updater invokes /usr/sbin/update-grub2 inside the payload chroot.
touch "$WORK/payload/.startos-migration"
install -m0755 "$SOURCE_DIR/lib/scripts/migration-update-grub" "$WORK/payload/usr/sbin/update-grub2"
install -m0755 "$SOURCE_DIR/lib/scripts/normalize-fstab" "$WORK/payload/usr/lib/startos/scripts/normalize-fstab"

rm -f "$OUT"
mksquashfs "$WORK/payload" "$OUT" -noappend -comp gzip -b 4096
if [ -n "${OWNER_UID:-}" ]; then chown "$OWNER_UID:${OWNER_GID:-$OWNER_UID}" "$OUT"; fi

echo "migration payload for $ARCH -> $OUT (base image $B3SUM.rootfs)"
