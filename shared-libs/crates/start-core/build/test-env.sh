#!/bin/bash

# Sourced from the repo root. Man-page export reuses the unit-test harness only while both
# callers pass `TEST_ARGS` unchanged.

set -ea

if [ "${RUST_BUILDER_IMAGE:-start9/start-core-testenv}" = "start9/start-core-testenv" ]; then
  base_image=$(env -u RUST_BUILDER_IMAGE ./build/rust-builder/image.sh)
  RUST_BUILDER_IMAGE=start9/start-core-testenv
  RUST_BUILDER_IMAGE=$(./build/docker-build-image.sh -t "$RUST_BUILDER_IMAGE" \
    --build-arg RUST_BUILDER_IMAGE="$base_image" \
    -f shared-libs/crates/start-core/testenv.Dockerfile .)
fi

source ./build/builder-alias.sh
source ./build/rust-build-profile.sh
shopt -s expand_aliases

set_test_profile

ARCH=${ARCH:-$(uname -m)}
if [ "$ARCH" = "arm64" ]; then
  ARCH=aarch64
fi

FEATURES="${ENVIRONMENT//-/,}"
RUSTFLAGS=""
if [[ "${ENVIRONMENT}" =~ (^|-)console($|-) ]]; then
  RUSTFLAGS="--cfg tokio_unstable"
fi

echo "FEATURES=\"$FEATURES\""
echo "RUSTFLAGS=\"$RUSTFLAGS\""

TEST_ARGS=(--manifest-path=./Cargo.toml "${BUILD_FLAGS[@]}" --features=test,ts,$FEATURES -p start-core -p exver --locked --lib)
