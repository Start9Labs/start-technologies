#!/bin/bash
set -euo pipefail

image_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
printf '%s\n' "${RUST_BUILDER_IMAGE:-$(<"$image_dir/repository")$(<"$image_dir/reference")}"
