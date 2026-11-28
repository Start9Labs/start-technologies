#!/bin/bash
set -euo pipefail
binary=$(realpath "${1:?start-core test executable required}")
tests=$(cd "$(dirname "$0")" && pwd)
image=$(mktemp)
trap 'rm -f "$image"' EXIT
docker build --load --iidfile "$image" -f "$tests/fde-profiles.Dockerfile" "$tests"
docker run --rm --network=none --cap-drop=ALL \
    --tmpfs /tmp:rw,nosuid,nodev,mode=1777 \
    --tmpfs /run:rw,nosuid,nodev,mode=755 \
    -v "$binary:/start-core-tests:ro" -e FDE_NM_TEST=1 \
    "$(<"$image")" bash -euc '
        NetworkManager --version
        name=fde::capture::tests::profiles::native_networkmanager_keeps_the_same_duplicate_uuid_winner_after_transport
        inventory=$(/start-core-tests --list)
        grep -Fxq -- "$name: test" <<< "$inventory"
        /start-core-tests --exact "$name" --ignored --nocapture
    '
