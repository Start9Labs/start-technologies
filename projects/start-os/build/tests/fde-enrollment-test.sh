#!/bin/bash
set -euo pipefail

directory=$(cd "$(dirname "$0")" && pwd)
binary=$(realpath "${1:?provide the start-core test executable}")
image_id=$(mktemp)
trap 'rm -f "$image_id"' EXIT
docker build --load --iidfile "$image_id" -f "$directory/fde-uki.Dockerfile" "$directory"
docker run --rm --network none --cap-drop=ALL --read-only \
    --tmpfs /tmp:rw,nosuid,nodev \
    --tmpfs /run:rw,nosuid,nodev \
    --tmpfs /dev/shm:rw,nosuid,nodev,size=64m \
    --mount "type=bind,source=$binary,target=/fde-tests,readonly" \
    "$(< "$image_id")" timeout --kill-after=10s 120s /fde-tests \
    --exact fde::enrollment::tests::native_signing_identities_are_separate_and_private \
    --ignored --nocapture
