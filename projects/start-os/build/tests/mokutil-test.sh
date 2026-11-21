#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../../.." && pwd)
image=$(mktemp)
trap 'rm -f "$image"' EXIT
docker build --load --iidfile "$image" -f "$root/projects/start-os/build/tests/mokutil.Dockerfile" "$root"
mounts=()
if [[ -n "${FDE_MOK_TEST_BINARY:-}" ]]; then
    binary=$(realpath "$FDE_MOK_TEST_BINARY")
    mounts+=(--mount "type=bind,source=$binary,target=/fde-tests,readonly" -e FDE_MOK_TEST_BINARY=/fde-tests)
fi
arguments=(--test)
if [[ -n "${FDE_MOK_TEST_PATTERN:-}" ]]; then
    arguments+=(--test-name-pattern "$FDE_MOK_TEST_PATTERN")
fi
docker run --rm --network none --cap-drop=ALL --read-only \
    --tmpfs /tmp:rw,nosuid,nodev --tmpfs /run:rw,nosuid,nodev \
    "${mounts[@]}" "$(< "$image")" "${arguments[@]}" /mokutil-test.mjs
