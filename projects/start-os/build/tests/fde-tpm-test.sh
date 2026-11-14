#!/bin/bash
set -euo pipefail

binary=$(realpath "${1:?path to the start-core unit-test executable}")
directory=$(cd "$(dirname "$0")" && pwd)
image=startos-fde-tpm-test
docker build --load -t "$image" -f "$directory/fde-tpm.Dockerfile" "$directory"
for test in \
    fde::tpm::tests::software_tpm_unseals_only_the_authorized_boot_state \
    fde::tpm::tests::software_tpm_rejects_external_stub_parameters \
    fde::tpm::tests::software_tpm_rejects_external_stub_extensions \
    fde::boot::tests::software_tpm_selects_only_compatible_generations \
    fde::credentials::tests::software_tpm_sealed_snapshot_round_trips_and_rejects_tampering; do
    docker run --rm --network none --tmpfs /var/tmp:rw,nosuid,nodev,mode=0700 \
        --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
        "$image" fde-tpm-run /fde-tests "$test"
done
test=fde::memory::tests::initramfs_protection_survives_exec_and_rechecks_dumpability
"$binary" --list | grep -Fx "$test: test"
docker run --rm --network none --tmpfs /var/tmp:rw,nosuid,nodev,mode=0700 \
    --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
    "$image" /fde-tests --ignored --exact "$test" --nocapture
