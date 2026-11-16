#!/bin/bash
set -euo pipefail
if [ "$#" -ne 3 ] || [ "$(id -u)" -ne 0 ]; then
    printf 'Usage in an exclusive managed guest: sudo unshare --mount --propagation private -- %s <genuine-os-squashfs> <current-scripts-directory> <output-directory>\n' "$0" >&2
    exit 1
fi
if [ "$(readlink /proc/self/ns/mnt)" = "$(readlink /proc/1/ns/mnt)" ]; then
    printf 'A private mount namespace is required\n' >&2
    exit 1
fi
image=$(realpath "$1")
scripts=$(realpath "$2")
output=$(realpath "$3")
work=$(mktemp -d)
mounts=()
cleanup() {
    local index
    for ((index=${#mounts[@]}-1; index>=0; index--)); do
        umount "${mounts[index]}" || return 1
    done
    rm -rf "$work"
}
trap cleanup EXIT
mount_view() {
    local destination=$1
    shift
    mount "$@" "$destination"
    mounts+=("$destination")
}
mkdir "$work/public" "$work/runtime" "$work/root"
mount_view "$work/public" -o loop,ro "$image"
mount_view "$work/runtime" -o loop,ro "$work/public/usr/lib/startos/container-runtime/rootfs.squashfs"
root="$work/root"
mount_view "$root" -t overlay -o "ro,lowerdir=$work/public:$work/runtime" overlay
for path in run tmp var/tmp dev; do
    mount_view "$root/$path" -t tmpfs -o nosuid,nodev,mode=0700 tmpfs
done
for device in null zero random urandom; do
    touch "$root/dev/$device"
    mount_view "$root/dev/$device" --bind "/dev/$device"
    mount -o remount,bind,ro "$root/dev/$device"
done
mount_view "$root/proc" -t proc -o ro,nosuid,nodev,noexec proc
mkdir "$root/tmp/current-fde-scripts"
mount_view "$root/tmp/current-fde-scripts" --bind "$scripts"
mount -o remount,bind,ro "$root/tmp/current-fde-scripts"
chroot "$root" /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin LC_ALL=C TMPDIR=/var/tmp HOME=/var/tmp \
    /usr/bin/node /tmp/current-fde-scripts/fde-assemble-initramfs.mjs /run/candidate
cp "$root/run/candidate" "$output/initrd"
chroot "$root" /usr/bin/unmkinitramfs /run/candidate /var/tmp/inspected
candidate="$root/var/tmp/inspected"
if [ -d "$candidate/early" ]; then
    candidate="$candidate/main"
fi
for path in usr/bin/startbox usr/bin/tpm2 usr/sbin/veritysetup scripts/fde-fail scripts/startos-fde; do
    test -f "$candidate/$path"
done
test ! -e "$candidate/scripts/startos"
test ! -e "$candidate/scripts/live"
test ! -e "$candidate/usr/lib/udev/rules.d/70-persistent-cd.rules"
test -f "$work/public/etc/udev/rules.d/70-persistent-cd.rules"
test ! -s "$work/public/etc/udev/rules.d/70-persistent-cd.rules"
versions=("$candidate/usr/lib/modules"/*)
test "${#versions[@]}" -eq 1
while read -r module; do
    chroot "$root" /usr/sbin/modprobe --show-depends --dirname "${candidate#"$root"}" --set-version "${versions[0]##*/}" "$module"
done < "$candidate/conf/modules"
cmp "$candidate/usr/bin/startbox" "$work/public/usr/bin/startbox"
find "$candidate/etc" -type f -printf '%P\n' | sort > "$output/etc-files.txt"
find "$candidate/usr/lib/modules" -type f -printf '%P\n' | sort > "$output/module-files.txt"
chroot "$root" /usr/bin/node --input-type=module - "${candidate#"$root"}" <<'NODE'
import assert from 'node:assert/strict'
import { unlink } from 'node:fs/promises'
import { join } from 'node:path'
import { admitRequiredBootModules } from '/tmp/current-fde-scripts/fde-initramfs-modules.mjs'
import { filesUnder, kernelDirectory } from '/tmp/current-fde-scripts/fde-initramfs-paths.mjs'
import { runSource } from '/tmp/current-fde-scripts/fde-reviewed-source.mjs'
const candidate = process.argv[2]
await admitRequiredBootModules(candidate)
const kernel = await kernelDirectory(candidate)
const verity = (await filesUnder(candidate, kernel.path)).filter(path => /\/dm-verity\.ko(?:\.(?:xz|gz|zst))?$/.test(path))
assert.equal(verity.length, 1, 'The negative fixture requires a modular dm_verity kernel')
await unlink(join(candidate, verity[0]))
await runSource('/usr/sbin/depmod', ['--all', `--basedir=${candidate}`, '--moduledir=/usr/lib/modules', kernel.version])
await assert.rejects(admitRequiredBootModules(candidate), /dm_verity/)
console.log('Native required-module admission rejects the valid subset without dm_verity')
NODE
find "$root/var/tmp" -maxdepth 1 -name '.fde-*' -print > "$output/remaining-workspaces.txt"
test ! -s "$output/remaining-workspaces.txt"
printf 'Native unsigned assembly and scoped cleanup passed\n'
