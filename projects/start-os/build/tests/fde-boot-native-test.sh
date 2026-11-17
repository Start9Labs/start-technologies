#!/bin/bash
set -euo pipefail
[ "${STARTOS_TEST_MANAGED_GUEST:-}" = 1 ] && [ "$(id -u)" = 0 ] || {
    echo 'Requires an exclusive managed guest and root; never run on the build host.' >&2
    exit 1
}
binary=$(realpath "${1:?start-core test executable}")
root=$(realpath "${2:?exported native TPM fixture root}")
shift 2
if [ "$#" = 0 ]; then
    set -- fde::boot::tests::software_tpm_initramfs_prepares_protected_bootstrap_handoff
fi
unshare --mount --propagation slave -- bash -s -- "$binary" "$root" "$@" <<'FIXTURE'
set -euo pipefail
binary=$1
root=$2
shift 2
trap 'umount --recursive "$root"' EXIT
mount --bind "$root" "$root"
for path in proc dev sys run tmp; do mkdir -p "$root/$path"; done
mount --rbind --make-rslave /dev "$root/dev"
mount -t sysfs -o ro,nosuid,nodev,noexec sysfs "$root/sys"
for path in run tmp; do mount -t tmpfs -o nosuid,nodev tmpfs "$root/$path"; done
touch "$root/fde-tests"
mount --bind -o ro "$binary" "$root/fde-tests"
for test do
    timeout --kill-after=15s 180s unshare --pid --net --fork --kill-child=SIGKILL -- bash -euc '
        mount -t proc proc "$1/proc"
        exec chroot "$1" fde-tpm-run /fde-tests "$2"
    ' native-tpm "$root" "$test"
    umount "$root/proc"
done
FIXTURE
