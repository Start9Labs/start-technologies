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
    fde::boot::tests::software_tpm_initramfs_failure_publishes_no_handoff \
    fde::boot::tests::software_tpm_root_composition_failure_unwinds_before_measurement; do
    docker run --rm --privileged --network none --tmpfs /var/tmp:rw,nosuid,nodev,mode=0700 \
        --mount type=bind,source=/dev,target=/dev \
        --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
        "$image" fde-tpm-run /fde-tests "$test"
done
for test in \
    disk::fde::keyring::tests::kernel_keys_outlive_producer_session_and_deny_unprivileged_reads \
    disk::fde::tests::real_luks_cached_keys_reset_after_unlock_process_exits \
    os_overlay::tests::busy_verity_close_retains_protection_and_defers_kernel_cleanup \
    os_overlay::tests::verity_mount_retains_protection_through_handoff_and_cleans_failures \
    os_overlay::tests::cancelled_native_verity_setup_closes_abandoned_mappings_and_mounts; do
    "$binary" --list | grep -Fx "$test: test"
    docker run --rm --privileged --network none \
        --mount type=bind,source=/dev,target=/dev \
        --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
        "$image" /fde-tests --ignored --exact "$test" --nocapture
done
