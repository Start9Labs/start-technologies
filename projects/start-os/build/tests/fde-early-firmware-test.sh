#!/bin/bash
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    printf 'Usage: %s <genuine-hook-provider-tree> <installed-initramfs-tools-prefix> [TPM-fixture-image]\n' "$0" >&2
    exit 1
fi
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source_root=$(realpath "$1")
prefix=$(realpath "$2")
image=$(docker image inspect --format '{{.Id}}' "${3:-startos-fde-tpm-test}")
temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT
node --input-type=module - "$here/../lib/scripts/fde-initramfs-hooks.mjs" "$source_root" "$temp/hooks" <<'JS'
const { prepareNativeHooks } = await import(process.argv[2])
await prepareNativeHooks(process.argv[3], process.argv[4])
JS
common=(--rm --pull=never -i
    --mount "type=bind,src=$temp/hooks,dst=/fde-hooks,readonly"
    --mount "type=bind,src=$prefix/usr/share/initramfs-tools/hook-functions,dst=/usr/share/initramfs-tools/hook-functions,readonly"
    --mount "type=bind,src=$source_root/etc/default/intel-microcode,dst=/etc/default/intel-microcode,readonly"
    --mount "type=bind,src=$source_root/etc/default/amd64-microcode,dst=/etc/default/amd64-microcode,readonly"
)
docker run "${common[@]}" \
    --mount "type=bind,src=$prefix/usr/bin/cpio,dst=/usr/bin/cpio,readonly" \
    --mount "type=bind,src=$source_root/usr/sbin/iucode_tool,dst=/usr/sbin/iucode_tool,readonly" \
    --mount "type=bind,src=$source_root/usr/lib/firmware,dst=/usr/lib/firmware,readonly" \
    "$image" bash -se <<'SH'
set -euo pipefail
export MODULES=most verbose=n version
version=$(uname -r)
export DESTDIR=/tmp/generated
mkdir "$DESTDIR"
for name in intel_microcode amd64_microcode; do
    export __TMPEARLYCPIO="/tmp/$name.cpio"
    : > "$__TMPEARLYCPIO"
    sh "/fde-hooks/$name" build
    if [ -s "$__TMPEARLYCPIO" ]; then
        cpio -it --quiet < "$__TMPEARLYCPIO" > "/tmp/$name.files"
        case "$name" in
            intel_microcode) grep -Fxq kernel/x86/microcode/GenuineIntel.bin "/tmp/$name.files" ;;
            amd64_microcode) grep -Eq '^\.?/?kernel/x86/microcode/AuthenticAMD.bin$' "/tmp/$name.files" ;;
        esac
        export __TMPEARLYCPIO=/tmp
        status=0
        sh "/fde-hooks/$name" build > /tmp/failure.stdout 2> /tmp/failure.stderr || status=$?
        test "$status" != 0
        grep -Fq 'failed to create or prepend' /tmp/failure.stderr
        supported_version=$version
        version=1
        status=0
        sh "/fde-hooks/$name" build > /tmp/kernel.stdout 2> /tmp/kernel.stderr || status=$?
        test "$status" != 0
        grep -Fq 'unsupported kernel version' /tmp/kernel.stderr
        version=$supported_version
    else
        test "$name" = amd64_microcode
        if grep -q AuthenticAMD /proc/cpuinfo; then
            printf 'AMD microcode unexpectedly absent\n' >&2
            exit 1
        fi
    fi
done
printf 'Genuine native microcode archives, failed-append and unsupported-version rejection passed; no firmware was uploaded.\n'
SH
docker run "${common[@]}" "$image" sh -se <<'SH'
export MODULES=most verbose=n version
version=$(uname -r)
test ! -x /usr/sbin/iucode_tool
status=0
sh /fde-hooks/intel_microcode build > /tmp/missing.stdout 2> /tmp/missing.stderr || status=$?
test "$status" != 0
grep -Fq 'cannot run iucode_tool' /tmp/missing.stderr
printf 'Missing native microcode helper rejects the candidate.\n'
SH
