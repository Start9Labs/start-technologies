#!/bin/bash
set -euo pipefail

binary=$(realpath "${1:?path to the start-core unit-test executable}")
shift
if [ "$#" = 0 ]; then
    set -- \
        disk::fde::tests::real_luks_keyslots_and_header_backups_retire_using_only_new_password \
        disk::fde::tests::real_luks_account_changes_resume_install_and_committed_retirement
fi
directory=$(cd "$(dirname "$0")" && pwd)
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
docker build --load --iidfile "$workspace/image-id" -f "$directory/fde-luks.Dockerfile" "$directory"
image=$(<"$workspace/image-id")
docker run --rm --network none --tmpfs /dev/shm:exec,nosuid,nodev,size=256m --tmpfs /tmp:exec,nosuid,nodev --tmpfs /run --tmpfs /media/startos/config \
    --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
    "$image" sh -eu -c '
        for test do
            /fde-tests --list | grep -Fx "$test: test"
            timeout --kill-after=15s 420s /fde-tests --ignored --exact "$test" --nocapture
        done
    ' fde-luks "$@"
