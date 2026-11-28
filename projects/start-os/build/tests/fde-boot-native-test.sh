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
boot_mappings() {
    local names name
    names=$(dmsetup info -c --noheadings -o name) || return
    while IFS= read -r name; do
        case "$name" in startos-fde-root-*|startos-fde-config-*) printf '%s\n' "$name";; esac
    done <<< "$names"
}
declare -A baseline=()
names=$(boot_mappings)
while IFS= read -r name; do [ -z "$name" ] || baseline["$name"]=1; done <<< "$names"
new_mappings() {
    local names name
    names=$(boot_mappings) || return
    while IFS= read -r name; do
        if [ -n "$name" ] && [ -z "${baseline[$name]+set}" ]; then printf '%s\n' "$name"; fi
    done <<< "$names"
}
cleanup() {
    local status=$? failure=0 names name
    names=$(new_mappings) || failure=$?
    while IFS= read -r name; do
        [ -z "$name" ] || dmsetup remove "$name" || failure=$?
    done <<< "$names"
    umount --recursive "$root" || failure=$?
    if [ "$status" != 0 ]; then return "$status"; fi
    return "$failure"
}
trap cleanup EXIT
mount --bind "$root" "$root"
for path in proc dev sys; do mkdir -p "$root/$path"; done
mount --rbind --make-rslave /dev "$root/dev"
mount -t sysfs -o ro,nosuid,nodev,noexec sysfs "$root/sys"
ram=(run tmp dev/shm var/tmp/startos)
for path in "${ram[@]}"; do mkdir -p "$root/$path"; done
touch "$root/fde-tests"
mount --bind -o ro "$binary" "$root/fde-tests"
for test do
    for path in "${ram[@]}"; do mount -t tmpfs -o nosuid,nodev tmpfs "$root/$path"; done
    timeout --kill-after=15s 180s unshare --mount --propagation slave --pid --net --fork --kill-child=SIGKILL -- bash -euc '
        mount -t proc proc "$1/proc"
        exec chroot "$1" fde-tpm-run /fde-tests "$2"
    ' native-tpm "$root" "$test"
    names=$(new_mappings)
    [ -z "$names" ] || { printf 'Fixture left verity mappings:\n%s\n' "$names" >&2; exit 1; }
    for path in "${ram[@]}"; do umount "$root/$path"; done
done
FIXTURE
