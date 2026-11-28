#!/bin/bash
set -euo pipefail
binary=$(realpath "${1:?start-core test executable}")
root=$(realpath "${2:?exported native LUKS fixture root; use an exclusive managed guest}")
shift 2
if [ "$#" = 0 ]; then
    set -- \
        disk::fde::tests::enable_tests::real_pending_enable_luks1_recovers_initial_administrator_and_password_changes \
        disk::fde::tests::enable_tests::real_pending_enable_luks2_recovers_initial_administrator_and_password_changes
fi
unshare --mount --propagation private -- bash -s -- "$binary" "$root" "$@" <<'FIXTURE'
set -euo pipefail
binary=$1
root=$2
shift 2
trap 'umount --recursive "$root"' EXIT
mount --bind "$root" "$root"
for path in proc dev run tmp; do mkdir -p "$root/$path"; done
mount -t proc proc "$root/proc"
mount -t tmpfs -o nosuid,noexec,mode=755 tmpfs "$root/dev"
for device in null zero random urandom; do
    touch "$root/dev/$device"
    mount --bind "/dev/$device" "$root/dev/$device"
done
mkdir "$root/dev/shm"
mount -t tmpfs -o nosuid,nodev,size=256m tmpfs "$root/dev/shm"
for path in run tmp; do mount -t tmpfs -o nosuid,nodev tmpfs "$root/$path"; done
touch "$root/fde-tests"
mount --bind -o ro "$binary" "$root/fde-tests"
for test do
    chroot "$root" /fde-tests --list | grep -Fx "$test: test"
    timeout --kill-after=15s 420s chroot "$root" /fde-tests --ignored --exact "$test" --nocapture
done
FIXTURE
