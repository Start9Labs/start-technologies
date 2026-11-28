#!/bin/bash
set -euo pipefail
directory=$(cd "$(dirname "$0")" && pwd)
docker build --load --iidfile "${1:?output image-ID file}" -f "$directory/fde-luks.Dockerfile" "$directory"
