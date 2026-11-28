#!/bin/bash
set -euo pipefail
if [ "${STARTOS_TEST_MANAGED_GUEST:-}" != 1 ] || [ "$(id -u)" != 0 ]; then
    printf 'Requires an exclusive managed guest and root mount privileges\n' >&2
    exit 1
fi
binary=$(realpath "${1:?start-core test executable}")
shift
names=(
    os_overlay::orphan_tests::native_orphan_recovery_rejects_mounted_and_uninspectable_candidates
    disk::mount::util::tests::native_failed_temporary_teardown_preserves_mounted_contents
)
for name in "${names[@]}"; do
    "$binary" --list | grep -Fx -- "$name: test"
done
if [ "$#" = 0 ]; then
    set -- "${names[@]}"
fi
for name in "$@"; do
    valid=0
    for expected in "${names[@]}"; do
        if [ "$name" = "$expected" ]; then valid=1; fi
    done
    if [ "$valid" != 1 ]; then
        printf 'Unknown native orphan-recovery case: %s\n' "$name" >&2
        exit 1
    fi
done
for name in "$@"; do
    unshare --mount --propagation private -- "$binary" --exact "$name" --ignored --nocapture
done
