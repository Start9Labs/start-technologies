#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

set -ea

RUST_BUILDER_IMAGE=${RUST_BUILDER_IMAGE:-start9/start-core-testenv}
if [ "$RUST_BUILDER_IMAGE" = "start9/start-core-testenv" ]; then
  docker build -t "$RUST_BUILDER_IMAGE" -f shared-libs/crates/start-core/testenv.Dockerfile .
fi

source ./build/builder-alias.sh
shopt -s expand_aliases

PROFILE=${PROFILE:-release}
BUILD_FLAGS=
case "$PROFILE" in
  release) BUILD_FLAGS="--release" ;;
  dev|debug) ;;
  *)
    >&2 echo "Unknown profile $PROFILE: falling back to debug..."
    PROFILE=debug
    ;;
esac

if [ -z "$ARCH" ]; then
	ARCH=$(uname -m)
fi

if [ "$ARCH" = "arm64" ]; then
  ARCH="aarch64"
fi

USE_TTY=
if tty -s; then
	USE_TTY="-it"
fi

FEATURES="$(echo $ENVIRONMENT | sed 's/-/,/g')"
RUSTFLAGS=""

if [[ "${ENVIRONMENT}" =~ (^|-)console($|-) ]]; then
	RUSTFLAGS="--cfg tokio_unstable"
fi


echo "FEATURES=\"$FEATURES\""
echo "RUSTFLAGS=\"$RUSTFLAGS\""
rust-zig-builder cargo test --manifest-path=./Cargo.toml $BUILD_FLAGS --features=test,$FEATURES -p start-core --locked --lib -- --skip export_
rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
