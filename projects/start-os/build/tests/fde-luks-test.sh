#!/bin/bash
set -euo pipefail

binary=$(realpath "${1:?path to the start-core unit-test executable}")
directory=$(cd "$(dirname "$0")" && pwd)
image=startos-fde-luks-test
docker build --load -t "$image" -f "$directory/fde-luks.Dockerfile" "$directory"
docker run --rm --network none --tmpfs /tmp:exec,nosuid,nodev --tmpfs /run \
    --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
    "$image" sh -eu -c '
        test=disk::fde::tests::real_luks_keyslots_and_header_backups_retire_using_only_new_password
        /fde-tests --list | grep -Fx "$test: test"
        exec /fde-tests --ignored --exact "$test" --nocapture
    '
