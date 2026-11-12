#!/bin/bash
set -euo pipefail

binary=$(realpath "${1:?path to the start-core unit-test executable}")
directory=$(cd "$(dirname "$0")" && pwd)
image=startos-fde-tpm-test
docker build --load -t "$image" -f "$directory/fde-tpm.Dockerfile" "$directory"
for test in \
    fde::boot::tests::software_tpm_mounted_generation_measures_before_unseal_and_closes \
    fde::boot::tests::software_tpm_mounted_generation_rejects_boot_change_before_measurement \
    fde::boot::tests::software_tpm_initramfs_prepares_protected_bootstrap_handoff \
    fde::boot::tests::software_tpm_initramfs_failure_publishes_no_handoff; do
    docker run --rm --privileged --network none --tmpfs /var/tmp:rw,nosuid,nodev,mode=0700 \
        --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
        "$image" fde-tpm-run /fde-tests "$test"
done
test=os_overlay::tests::verity_mount_retains_protection_through_handoff_and_cleans_failures
"$binary" --list | grep -Fx "$test: test"
docker run --rm --privileged --network none \
    --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
    "$image" /fde-tests --ignored --exact "$test" --nocapture
