#!/bin/bash
# Run inside a disposable StartOS VM: fileMounts.vm.sh NODE START_CONTAINER BUNDLE
set -euo pipefail
systemd-detect-virt --vm --quiet
[[ $(id -u) == 0 ]]
NODE=$(realpath "$1")
HELPER=$(realpath "$2")
BUNDLE=$(realpath "$3")
if [[ ${FILE_MOUNT_TEST_NAMESPACE:-} != 1 ]]; then
    exec env FILE_MOUNT_TEST_NAMESPACE=1 unshare --mount --propagation private bash "$0" "$NODE" "$HELPER" "$BUNDLE"
fi
SOURCE=/media/startos/volumes/file-watch-test
[[ ! -e "$SOURCE" && ! -e /media/startos/assets/file-watch-test.txt ]]
[[ ! -e /media/startos/images/file-watch-test.json && ! -e /media/startos/images/file-watch-test.env ]]
BASE=$(mktemp -d /var/tmp/file-mounts.XXXXXX)
ROOTFS="$BASE/rootfs"
CREATED_HELPER=0
cleanup() {
    umount /usr/bin/start-container 2>/dev/null || true
    if [[ $CREATED_HELPER == 1 ]]; then rm -f /usr/bin/start-container; fi
    umount -R "$ROOTFS" 2>/dev/null || true
    umount "$SOURCE" 2>/dev/null || true
    umount "$BASE/runtime" 2>/dev/null || true
    rm -rf "$BASE" "$SOURCE"
    rm -f /media/startos/images/file-watch-test.json /media/startos/images/file-watch-test.env /media/startos/assets/file-watch-test.txt
}
trap cleanup EXIT
mkdir -p "$BASE/upper" "$BASE/work" "$ROOTFS" "$BASE/runtime" "$BASE/bin" "$SOURCE"
ln -s "$HELPER" "$BASE/bin/start-container"
export PATH="$BASE/bin:$PATH"
if [[ ! -e /usr/bin/start-container ]]; then
    touch /usr/bin/start-container
    CREATED_HELPER=1
fi
mount --bind "$HELPER" /usr/bin/start-container
mount -t squashfs -o loop,ro /usr/lib/startos/container-runtime/rootfs.squashfs "$BASE/runtime"
mount -t tmpfs file-test "$SOURCE"
mount -t overlay overlay -o "lowerdir=$BASE/runtime,upperdir=$BASE/upper,workdir=$BASE/work" "$ROOTFS"
"$NODE" "$BUNDLE" "$ROOTFS"
