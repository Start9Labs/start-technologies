#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

source ./build/builder-alias.sh

set -ea

INSTALL=false
while [[ $# -gt 0 ]]; do
  case $1 in
    --install)
      INSTALL=true
      shift
      ;;
    *)
      >&2 echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

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

if [ -z "${ARCH:-}" ]; then
  ARCH=$(uname -m)
fi

if [ "$ARCH" = "arm64" ]; then
  ARCH="aarch64"
fi

RUST_ARCH="$ARCH"
if [ "$ARCH" = "riscv64" ]; then
  RUST_ARCH="riscv64gc"
fi

if [ -z "${KERNEL_NAME:-}" ]; then
  KERNEL_NAME=$(uname -s)
fi

if [ -z "${TARGET:-}" ]; then
  if [ "$KERNEL_NAME" = "Linux" ]; then
    TARGET="$RUST_ARCH-unknown-linux-musl"
  elif [ "$KERNEL_NAME" = "Darwin" ]; then
    TARGET="$RUST_ARCH-apple-darwin"
  else
    >&2 echo "unknown kernel $KERNEL_NAME"
    exit 1
  fi
fi

FEATURES="$(echo "${ENVIRONMENT:-}" | sed 's/-/,/g')"
RUSTFLAGS=""
if [[ "${ENVIRONMENT:-}" =~ (^|-)console($|-) ]]; then
  RUSTFLAGS="--cfg tokio_unstable"
fi

if [[ "${ENVIRONMENT:-}" =~ (^|-)unstable($|-) ]]; then
  RUSTFLAGS="$RUSTFLAGS -C debuginfo=1"
fi

echo "FEATURES=\"$FEATURES\""
echo "RUSTFLAGS=\"$RUSTFLAGS\""
rust-zig-builder cargo zigbuild --manifest-path=./Cargo.toml $BUILD_FLAGS --features=$FEATURES --locked -p start-cli --bin start-cli --target=$TARGET
if [ "$(ls -nd "target/$TARGET/$PROFILE/start-cli" | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID  /usr/local/cargo"
fi

if [ "$INSTALL" = "true" ]; then
  cp "target/$TARGET/$PROFILE/start-cli" ~/.cargo/bin/start-cli
fi