#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

source ./build/builder-alias.sh

set -ea
shopt -s expand_aliases

PROFILE=${PROFILE:-release}
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

RUSTFLAGS="-C target-feature=+crt-static"

rust-zig-builder cargo zigbuild --manifest-path=./Cargo.toml $BUILD_FLAGS --locked -p pi-beep --bin pi-beep --target=$RUST_ARCH-unknown-linux-musl
if [ "$(ls -nd "target/$RUST_ARCH-unknown-linux-musl/$PROFILE/pi-beep" | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID  /usr/local/cargo"
fi
