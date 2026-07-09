#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."

source ./build/builder-alias.sh

set -ea
shopt -s expand_aliases

PROFILE=${PROFILE:-debug}
if [ "${PROFILE}" = "release" ]; then
	BUILD_FLAGS="--release"
else
  if [ "$PROFILE" != "debug" ]; then
    >&2 echo "Unknown profile $PROFILE: falling back to debug..."
    PROFILE=debug
  fi
fi

if [ -z "$ARCH" ]; then
	ARCH=$(uname -m)
fi

if [ "$ARCH" = "arm64" ]; then
	ARCH="aarch64"
fi

RUST_ARCH="$ARCH"
if [ "$ARCH" = "riscv64" ]; then
	RUST_ARCH="riscv64gc"
fi

FEATURES="$(echo $ENVIRONMENT | sed 's/-/,/g')"
RUSTFLAGS=""
if [[ "${ENVIRONMENT}" =~ (^|-)console($|-) ]]; then
	RUSTFLAGS="--cfg tokio_unstable"
fi
echo "FEATURES=\"$FEATURES\""
echo "RUSTFLAGS=\"$RUSTFLAGS\""
rust-zig-builder cargo test --manifest-path=./Cargo.toml -p start-core --lib $BUILD_FLAGS --features test,$FEATURES --locked 'export_bindings_'
if [ "$(ls -nd "shared-libs/crates/start-core/bindings" | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID shared-libs/crates/start-core/bindings && chown -R $UID:$UID  /usr/local/cargo"
fi