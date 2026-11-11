#!/bin/bash
set -euo pipefail

binary=$(realpath "${1:?path to the start-core unit-test executable}")
directory=$(cd "$(dirname "$0")" && pwd)
image=startos-fde-tpm-test
docker build -t "$image" -f "$directory/fde-tpm.Dockerfile" "$directory"
docker run --rm --network none \
    --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
    "$image" sh -eu -c '
        mkdir -m 700 /tmp/tpm
        swtpm socket --tpm2 --tpmstate dir=/tmp/tpm \
            --server type=unixio,path=/tmp/tpm/socket \
            --ctrl type=unixio,path=/tmp/tpm/socket.ctrl \
            --flags not-need-init --daemon
        TPM2TOOLS_TCTI=swtpm:path=/tmp/tpm/socket tpm2_startup -c
        mkdir -p /run/dbus
        dbus-daemon --system --fork
        tpm2-abrmd --allow-root --tcti=swtpm:path=/tmp/tpm/socket &
        manager=$!
        export TPM2TOOLS_TCTI=tabrmd:
        for attempt in $(seq 1 100); do
            if tpm2_getcap properties-fixed >/dev/null 2>&1; then
                break
            fi
            kill -0 "$manager"
            if [ "$attempt" = 100 ]; then
                exit 1
            fi
            sleep 0.05
        done
        exec /fde-tests --ignored --exact \
            fde::tpm::tests::software_tpm_unseals_only_the_authorized_boot_state --nocapture
    '
