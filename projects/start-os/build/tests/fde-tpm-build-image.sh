#!/bin/bash
set -euo pipefail

directory=$(cd "$(dirname "$0")" && pwd)
docker build --load --iidfile "${1:?provide the per-invocation image-ID file}" \
    -t startos-fde-tpm-test -f "$directory/fde-tpm.Dockerfile" "$directory"
