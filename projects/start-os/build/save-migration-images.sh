#!/bin/bash
set -euo pipefail

DESTDIR="${1:-projects/start-os/build/lib/migration-images}"

mkdir -p "$DESTDIR"
rm -f "$DESTDIR/start9_compat_latest.tar" "$DESTDIR/start9_utils_latest.tar" "$DESTDIR/tonistiigi_binfmt_latest.tar"

for ARCH in x86_64 aarch64; do
    case "$ARCH" in
        x86_64) DOCKER_PLATFORM="linux/amd64" ;;
        aarch64) DOCKER_PLATFORM="linux/arm64" ;;
    esac
    mkdir -p "$DESTDIR/$ARCH"
    for NAME in compat utils; do
        IMAGE="start9/$NAME:latest"
        OUTPUT="$DESTDIR/$ARCH/start9_${NAME}_latest.tar"
        if [ -f "$OUTPUT" ]; then
            echo "Skipping $IMAGE for $ARCH (already saved)"
            continue
        fi
        echo "Pulling $IMAGE for $DOCKER_PLATFORM..."
        docker pull --platform "$DOCKER_PLATFORM" "$IMAGE"
        echo "Saving $IMAGE to $OUTPUT..."
        docker save --platform "$DOCKER_PLATFORM" "$IMAGE" -o "$OUTPUT.tmp"
        mv "$OUTPUT.tmp" "$OUTPUT"
    done
done

echo "Migration images saved to $DESTDIR"
