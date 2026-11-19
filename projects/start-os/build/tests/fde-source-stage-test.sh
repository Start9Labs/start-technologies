#!/bin/bash
set -euo pipefail
if [ "${STARTOS_TEST_MANAGED_GUEST:-}" != 1 ] || [ "$(< /proc/1/comm)" != systemd ] || [ "$EUID" -ne 0 ]; then
    echo 'Requires an exclusive managed StartOS guest; never run on the build host.' >&2
    exit 1
fi
binary=$(realpath -e "${1:?test executable}")
startbox=$(realpath -e "${2:?current startbox}")
image=$(realpath -e "${3:?genuine pre-v2 source fixture}")
scripts=$(realpath -e "${4:?current build/lib/scripts}")
shift 4
allowed=(
    os_overlay::public_source::workspace_tests::native_workspace_recovery_retires_stale_mounts_and_only_its_reserved_mapping
    fde::public_image::tests::native_execution_root_excludes_parent_descriptors_but_preserves_standard_io
    fde::public_image::tests::native_public_source_worker_failure_releases_private_layers
)
if [ "$#" = 0 ]; then set -- "${allowed[@]}"; fi
inventory=$("$binary" --list)
for name in "$@"; do
    printf '%s\n' "${allowed[@]}" | grep -Fxq -- "$name"
    grep -Fxq -- "$name: test" <<< "$inventory"
done
root=$(mktemp -d /media/startos/images/.fde-source-stage.XXXXXXXX)
fixture=
canary=
cleanup() {
    local result=$?
    trap - EXIT
    if [ -n "$canary" ]; then
        veritysetup close "$canary" 2>/dev/null || true
    fi
    if [ -n "$fixture" ]; then
        (
            source "$scripts/os-image-store"
            startos_reserve_image_store /media/startos/config
            read -r tool < "$scripts/os-public-source-prune-name"
            "$startbox" "$tool" "$fixture"
        ) || result=1
    fi
    if findmnt -rn -o TARGET | grep -Fq -- "$root/"; then
        echo "Fixture mounts remain beneath $root" >&2
        exit 1
    fi
    if [ "$result" = 0 ]; then
        rm -rf -- "$root"
    else
        echo "Failed fixture retained: $root" >&2
    fi
    exit "$result"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
for name in "$@"; do
    fixture="$root/$(< /proc/sys/kernel/random/uuid)"
    mkdir -m 700 "$fixture"
    canary="fde-source-canary-$(< /proc/sys/kernel/random/uuid)"
    FDE_SOURCE_STAGE_ROOT="$fixture" FDE_SOURCE_STAGE_STARTBOX="$startbox" \
        FDE_SOURCE_STAGE_CANARY="$canary" FDE_SOURCE_IMAGE="$image" \
        timeout --kill-after=20s 240s "$binary" --exact "$name" --ignored --nocapture
    fixture=
    canary=
done
