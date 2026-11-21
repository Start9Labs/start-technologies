#!/bin/bash
set -euo pipefail

iid_file=$(mktemp)
trap 'rm -f "$iid_file"' EXIT

docker buildx build --load --iidfile "$iid_file" "$@" >&2
image=$(<"$iid_file")
if ! [[ "$image" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    printf 'Docker build returned an invalid image ID\n' >&2
    exit 1
fi
printf '%s\n' "$image"
