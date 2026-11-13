#!/bin/bash
set -euo pipefail

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    printf 'Usage: %s <genuine-os-root> <genuine-node-runtime-tree> <genuine-hook-sources> [TPM-fixture-image]\n' "$0" >&2
    exit 1
fi
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(realpath "$here/../../../..")
source_root=$(realpath "$1")
runtime=$(realpath "$2")
hook_sources=$(realpath "$3")
share=$(mktemp -d)
trap 'rm -rf "$share"' EXIT
cp -a "$runtime/usr/share/nodejs" "$share/nodejs"
cp -a "$source_root/usr/share/initramfs-tools" "$share/initramfs-tools"
image=$(docker image inspect --format '{{.Id}}' "${4:-startos-fde-tpm-test}")
docker run --rm --pull=never -i \
    --mount "type=bind,src=$source_root/usr,dst=/usr,readonly" \
    --mount "type=bind,src=$share,dst=/usr/share,readonly" \
    --mount "type=bind,src=$runtime,dst=/node-runtime,readonly" \
    --mount "type=bind,src=$hook_sources,dst=/fde-hook-sources,readonly" \
    --mount "type=bind,src=$repo,dst=/fde-repo,readonly" \
    --tmpfs /var/tmp:rw,exec,nosuid,nodev,mode=0700 \
    --mount type=bind,src=/dev/null,dst=/dev/null,readonly \
    --mount type=bind,src=/dev/random,dst=/dev/random,readonly \
    --mount type=bind,src=/dev/urandom,dst=/dev/urandom,readonly \
    "$image" /bin/bash -se <<'SH'
set -euo pipefail
export PATH=/node-runtime/usr/bin:/usr/sbin:/usr/bin:/sbin:/bin
export LD_LIBRARY_PATH=/node-runtime/usr/lib/x86_64-linux-gnu
node /fde-repo/projects/start-os/build/tests/fde-providers-test.mjs /fde-hook-sources
SH
