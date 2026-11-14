#!/bin/bash
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    printf 'Usage: %s <genuine-target-root> <genuine-startbox> [TPM-fixture-image]\n' "$0" >&2
    exit 1
fi
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source_root=$(realpath "$1")
startbox=$(realpath "$2")
modules=("$source_root"/usr/lib/modules/*)
test "${#modules[@]}" = 1
test -d "${modules[0]}"
version=$(basename "${modules[0]}")
test -s "$source_root/boot/vmlinuz-$version"
test -s "$source_root/boot/config-$version"
image=$(docker image inspect --format '{{.Id}}' "${3:-startos-fde-tpm-test}")
docker run --rm --pull=never -i --env "version=$version" \
    --mount "type=bind,src=${modules[0]},dst=/usr/lib/modules/$version,readonly" \
    --mount "type=bind,src=$source_root/usr/share/initramfs-tools/hook-functions,dst=/usr/share/initramfs-tools/hook-functions,readonly" \
    --mount "type=bind,src=$source_root/usr/lib/dracut/dracut-install,dst=/usr/lib/dracut/dracut-install,readonly" \
    --mount "type=bind,src=$source_root/usr/bin/kmod,dst=/usr/sbin/depmod,readonly" \
    --mount "type=bind,src=$source_root/usr/bin/kmod,dst=/usr/sbin/modprobe,readonly" \
    --mount "type=bind,src=$startbox,dst=/usr/bin/startbox,readonly" \
    --mount "type=bind,src=$here/../lib/scripts/fde-initramfs-tools-hook,dst=/fde-tools-hook,readonly" \
    "$image" bash -se <<'SH'
set -euo pipefail
export LC_ALL=C verbose=n DESTDIR=/fde-module-root
export __MODULES_TO_ADD=/tmp/fde-modules
: > "$__MODULES_TO_ADD"
mkdir -p "$DESTDIR"/{usr/{bin,sbin,lib,lib64},etc}
for dir in bin sbin lib lib64; do
    ln -s "usr/$dir" "$DESTDIR/$dir"
done
source_modules="/lib/modules/$version"
candidate_modules="$DESTDIR$source_modules"
mkdir -p "$candidate_modules"
for metadata in modules.builtin modules.builtin.bin modules.builtin.modinfo modules.order; do
    cp -p "$source_modules/$metadata" "$candidate_modules/$metadata"
done
before=$(find "$source_modules" -type f -exec sha256sum {} + | sort)
sh /fde-tools-hook
. /usr/share/initramfs-tools/hook-functions
apply_add_modules
test ! -s "$__MODULES_TO_ADD"
count=0
while IFS= read -r -d '' file; do
    cmp "$file" "$DESTDIR$file"
    count=$((count + 1))
done < <(find "$source_modules/kernel/drivers/char/tpm" -type f -name '*.ko*' -print0)
test "$count" -gt 0
test -s "$candidate_modules/kernel/drivers/char/tpm/st33zp24/tpm_st33zp24_i2c.ko.xz"
test -s "$candidate_modules/kernel/drivers/char/tpm/st33zp24/tpm_st33zp24.ko.xz"
depmod -a -b "$DESTDIR" "$version"
modprobe --dirname "$DESTDIR" --set-version "$version" --show-depends tpm_st33zp24_i2c > /tmp/module-dependencies
grep -Fq '/tpm_st33zp24.ko.xz' /tmp/module-dependencies
grep -Fq '/tpm_st33zp24_i2c.ko.xz' /tmp/module-dependencies
test "$(find "$source_modules" -type f -exec sha256sum {} + | sort)" = "$before"
printf 'Native production-hook collection/dependency copying preserved %s genuine TPM modules; native depmod/modprobe resolved their dependencies without insertion.\n' "$count"
SH
