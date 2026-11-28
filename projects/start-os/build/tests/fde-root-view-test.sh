#!/bin/bash
set -euo pipefail
if [ "${STARTOS_TEST_MANAGED_GUEST:-}" != 1 ] || [ "$EUID" -ne 0 ]; then
    echo 'Requires an exclusive managed StartOS guest; never run on the build host.' >&2
    exit 1
fi
if [ "$#" -lt 1 ]; then
    echo 'Usage: fde-root-view-test.sh <test-executable> [test-name...]' >&2
    exit 1
fi
binary=$(realpath -e -- "$1")
shift
name=fde::capture::tests::root_views::native_isolated_base_view_preserves_merged_references_without_source_submounts
published=fde::capture::tests::published_references::native_configuration_union_retains_published_gzip_and_zstd_without_boot_authority
pinned=os_overlay::rootfs_pin_tests::native_rootfs_retention_crosses_readonly_image_and_writable_config_bind_views
unused=disk::mount::guard::source_reservation_tests::native_source_removal_rejects_residual_cache_mounts_and_uninspectable_subtrees
if [ "$#" = 0 ]; then set -- "$name"; fi
inventory=$("$binary" --list)
for selected in "$@"; do
    if ! { [ "$selected" = "$name" ] || [ "$selected" = "$published" ] || [ "$selected" = "$pinned" ] || [ "$selected" = "$unused" ]; } || ! grep -Fxq -- "$selected: test" <<< "$inventory"; then
        echo "Unknown exact test: $selected" >&2
        exit 1
    fi
done
root=$(mktemp -d /var/tmp/startos-fde-root-view.XXXXXXXX)
trap 'rm -rf -- "$root"' EXIT
for selected in "$@"; do
    STARTOS_TEST_MANAGED_GUEST=1 FDE_ROOT_VIEW_TEST_ROOT="$root" RUST_MIN_STACK="${RUST_MIN_STACK:-8388608}" \
        timeout --kill-after=20s 120s unshare --mount --propagation private -- \
        "$binary" --ignored --exact "$selected" --nocapture
done
