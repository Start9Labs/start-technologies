#!/bin/bash

set -e

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

USE_TTY=
if tty -s; then
  USE_TTY="-it"
fi

DOCKER_PLATFORM=linux/${ARCH}
case $ARCH in
    x86_64)
        DOCKER_PLATFORM=linux/amd64;;
    aarch64)
        DOCKER_PLATFORM=linux/arm64;;
esac

docker run --rm $USE_TTY --platform=$DOCKER_PLATFORM -eARCH --privileged -v "$(pwd):/root/start-os" start9/build-env /root/start-os/projects/start-os/container-runtime/update-image.sh
if [ "$(ls -nd "projects/start-os/container-runtime/rootfs.${ARCH}.squashfs" | awk '{ print $3 }')" != "$UID" ]; then
  docker run --rm $USE_TTY -v "$(pwd):/root/start-os" start9/build-env chown -R $UID:$UID /root/start-os/projects/start-os/container-runtime
fi