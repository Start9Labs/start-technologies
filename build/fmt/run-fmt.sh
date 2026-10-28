#!/bin/bash
set -e

pwd=$(pwd)
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
repo_root=$(pwd)
rel_pwd="${pwd#"$repo_root"}"
TOOLCHAIN=$(sed -n 's/^ARG RUSTFMT_TOOLCHAIN=//p' build/fmt/fmtenv.Dockerfile)

if [ "${FMT_NATIVE:-}" = 1 ]; then
    exec env RUSTUP_TOOLCHAIN="$TOOLCHAIN" "$@"
fi

platform=linux/amd64
case "$(uname -m)" in
    aarch64 | arm64) platform=linux/arm64 ;;
esac

IMAGE=start9/fmt-env
base_image=$(./build/rust-builder/image.sh)
docker buildx build --load --platform="$platform" -t "$IMAGE" \
    --build-arg RUST_BUILDER_IMAGE="$base_image" \
    -f build/fmt/fmtenv.Dockerfile build/fmt

tty_args=()
if tty -s; then tty_args=(-it); fi

exec docker run "${tty_args[@]}" --platform="$platform" --rm \
    --user "$(id -u):$(id -g)" \
    -e RUSTUP_TOOLCHAIN="$TOOLCHAIN" \
    -v "$repo_root:/workdir" \
    -w "/workdir${rel_pwd}" \
    "$IMAGE" "$@"
