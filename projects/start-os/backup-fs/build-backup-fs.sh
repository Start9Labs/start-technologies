#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

source ./build/builder-alias.sh

set -ea
shopt -s expand_aliases

set_build_profile

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

rust-zig-builder cargo zigbuild --manifest-path=./Cargo.toml $BUILD_FLAGS --locked -p startos-backup-fs --bin startos-backup-fs --target=$RUST_ARCH-unknown-linux-musl
if [ "$(ls -nd "target/$RUST_ARCH-unknown-linux-musl/$PROFILE/startos-backup-fs" | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID  /usr/local/cargo"
fi
