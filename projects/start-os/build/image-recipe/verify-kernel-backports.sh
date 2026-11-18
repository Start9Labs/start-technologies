#!/bin/bash
set -euo pipefail
ROOT=$(realpath "$1")
ARCH=$2
source "$(dirname "${BASH_SOURCE[0]}")/kernel-backports.conf"
VERSION=$(dpkg-query --admindir="$ROOT/var/lib/dpkg" -W -f='${Version}' "linux-image-$ARCH")
dpkg --compare-versions "$VERSION" ge "$KERNEL_BACKPORT_VERSION"
mapfile -t BINARIES < <(find "$ROOT/usr/lib/modules" -mindepth 2 -maxdepth 2 -name vmlinuz -type f)
if [ "${#BINARIES[@]}" -ne 1 ]; then
    echo 'Expected one image-selected kernel binary' >&2
    exit 1
fi
KVER=$(basename "$(dirname "${BINARIES[0]}")")
cmp "${BINARIES[0]}" "$ROOT/boot/vmlinuz-$KVER"
grep -Fxq "GRUB_TOP_LEVEL=\"/boot/vmlinuz-$KVER\"" "$ROOT/etc/default/grub"
printf 'Verified image kernel %s, package %s\n' "$KVER" "$VERSION"
