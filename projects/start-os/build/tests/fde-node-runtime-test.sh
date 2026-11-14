#!/bin/bash
set -euo pipefail
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    printf 'Usage: %s <genuine-os-root> <genuine-node-runtime-tree> <reviewed-providers> [TPM-fixture-image]\n' "$0" >&2
    exit 1
fi
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(realpath "$here/../../../..")
os=$(realpath "$1")
runtime=$(realpath "$2")
providers=$(realpath "$3")
image=$(docker image inspect --format '{{.Id}}' "${4:-startos-fde-tpm-test}")
share=$(mktemp -d)
trap 'rm -rf "$share"' EXIT
cp -a "$runtime/usr/share/nodejs" "$share/nodejs"
cp -a "$os/usr/share/initramfs-tools" "$share/initramfs-tools"
docker run --rm --pull=never -i \
    --mount "type=bind,src=$os/usr,dst=/usr,readonly" \
    --mount "type=bind,src=$os/var/lib/dpkg,dst=/var/lib/dpkg,readonly" \
    --mount "type=bind,src=$share,dst=/usr/share,readonly" \
    --mount "type=bind,src=$runtime,dst=/node-runtime,readonly" \
    --mount "type=bind,src=$providers,dst=/fde-providers,readonly" \
    --mount "type=bind,src=$repo,dst=/fde-repo,readonly" \
    "$image" /bin/bash -se <<'SH'
set -euo pipefail
source /fde-repo/projects/start-os/build/tests/fde-node-runtime-env.sh
cd /fde-repo
node projects/start-os/build/tests/fde-node-runtime-test.mjs /fde-providers /node-runtime/usr/bin/node "$LD_LIBRARY_PATH"
node projects/start-os/build/tests/fde-initramfs-archive-test.mjs
node projects/start-os/build/tests/fde-initramfs-producer-test.mjs / /fde-providers
SH
