#!/bin/bash
# Run rustfmt inside start9/fmt-env (start9/cargo-zigbuild + a pinned nightly),
# so its output is identical for every dev and for CI regardless of the host's
# rolling `nightly`. prettier and taplo are pinned via npm and run natively —
# only rustfmt needs this. Set FMT_NATIVE=1 to run on the host instead (needs
# the pinned nightly installed).

set -e

pwd=$(pwd)
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
repo_root=$(pwd)
rel_pwd="${pwd#"$repo_root"}"

# Single source of truth for the pinned nightly: the image's build arg.
TOOLCHAIN=$(sed -n 's/^ARG RUSTFMT_TOOLCHAIN=//p' build/fmt/fmtenv.Dockerfile)

if [ "$FMT_NATIVE" = 1 ]; then
    # Pin the toolchain so a host whose default is stable still runs the right
    # nightly rustfmt (stable silently ignores our unstable options).
    exec env RUSTUP_TOOLCHAIN="$TOOLCHAIN" "$@"
fi

arch=$(uname -m)
platform=linux/amd64
case "$arch" in
    aarch64 | arm64) platform=linux/arm64 ;;
esac

IMAGE=start9/fmt-env

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "Building $IMAGE (one-time; pins the rustfmt nightly)..." >&2
    docker build --platform="$platform" -t "$IMAGE" \
        -f build/fmt/fmtenv.Dockerfile build/fmt
fi

if tty -s; then USE_TTY="-it"; fi

# --user keeps formatted files owned by the host user; RUSTUP_TOOLCHAIN selects
# the pinned nightly without relying on the base image's default toolchain.
exec docker run $USE_TTY --platform="$platform" --rm \
    --user "$(id -u):$(id -g)" \
    -e RUSTUP_TOOLCHAIN="$TOOLCHAIN" \
    -v "$repo_root:/workdir" \
    -w "/workdir${rel_pwd}" \
    "$IMAGE" "$@"
