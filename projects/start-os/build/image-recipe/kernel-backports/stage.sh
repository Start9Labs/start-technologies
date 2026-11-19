#!/bin/bash
set -euo pipefail
SUITE=$1
PLATFORM=$2
ARCH=$3
RESULTS_DIR=$4
SOURCE_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")
source "$SOURCE_DIR/kernel-backports.conf"
case "$SUITE:${PLATFORM%%-*}" in
    trixie:x86_64|trixie:aarch64)
        ARCHIVE="$RESULTS_DIR/kernel-backports/$ARCH"
        "$SOURCE_DIR/build-kernel-backports.sh" "$ARCH" "$ARCHIVE"
        mkdir -p "$(dirname "$KERNEL_BACKPORT_ARCHIVE_LIST")"
        printf 'deb [trusted=yes] file://%s ./\n' "$ARCHIVE/$KERNEL_BACKPORT_VERSION" \
            > "$KERNEL_BACKPORT_ARCHIVE_LIST"
        ;;
esac
