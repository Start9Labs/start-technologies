#!/bin/bash
cd "$(dirname "${BASH_SOURCE[0]}")"
set -e

DISTRO=debian
VERSION=trixie
ARCH=${ARCH:-$(uname -m)}
FLAVOR=default

_ARCH=$ARCH
if [ "$_ARCH" = "x86_64" ]; then
    _ARCH=amd64
elif [ "$_ARCH" = "aarch64" ]; then
    _ARCH=arm64
fi

RETRY=(--retry 5 --retry-delay 2 --retry-all-errors)

BASE_URL="https://images.linuxcontainers.org$(curl -fsSL "${RETRY[@]}" https://images.linuxcontainers.org/meta/1.0/index-system | grep "^$DISTRO;$VERSION;$_ARCH;$FLAVOR;" | head -n1 | sed 's/^.*;//g')"
OUTPUT_FILE="debian.${ARCH}.squashfs"

echo "Downloading ${BASE_URL}/rootfs.squashfs to $OUTPUT_FILE"
# -o, not a shell redirect: curl rewinds a file it owns before each retry, but
# cannot rewind stdout, so `> file` concatenates every partial attempt instead.
curl -fsSL "${RETRY[@]}" -o "$OUTPUT_FILE" "${BASE_URL}/rootfs.squashfs"
curl -fsSL "${RETRY[@]}" "$BASE_URL/SHA256SUMS" | grep 'rootfs\.squashfs' | awk '{print $1"  '"$OUTPUT_FILE"'"}' | shasum -a 256 -c