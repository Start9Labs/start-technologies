#!/bin/bash

# Builds each <package>:<bin> pair for one target in a single cargo invocation; shared crates
# compile once. TARGET defaults to the musl triple for ARCH.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

source ./build/builder-alias.sh

set -ea
shopt -s expand_aliases

if [ "$#" -eq 0 ]; then
  >&2 echo "usage: $0 <package>:<bin>..."
  exit 2
fi

PROFILE=${PROFILE:-release}
BUILD_FLAGS=()
if [ "$PROFILE" = release ]; then
  BUILD_FLAGS=(--release)
elif [ "$PROFILE" != debug ]; then
  >&2 echo "Unknown profile $PROFILE: falling back to debug..."
  PROFILE=debug
fi

ARCH=${ARCH:-$(uname -m)}
if [ "$ARCH" = arm64 ]; then
  ARCH=aarch64
fi
TARGET=${TARGET:-${ARCH/riscv64/riscv64gc}-unknown-linux-musl}
# The builder alias selects its sysroot from ARCH.
ARCH=${TARGET%%-*}
ARCH=${ARCH/riscv64gc/riscv64}

FEATURES="${ENVIRONMENT//-/,}"
RUSTFLAGS=""
if [[ "${ENVIRONMENT}" =~ (^|-)console($|-) ]]; then
  RUSTFLAGS="--cfg tokio_unstable"
fi
if [[ "${ENVIRONMENT}" =~ (^|-)unstable($|-) ]]; then
  RUSTFLAGS="$RUSTFLAGS -C debuginfo=1"
fi

PACKAGES=()
BINS=()
for pair in "$@"; do
  package=${pair%%:*}
  [[ " ${PACKAGES[*]} " == *" -p $package "* ]] || PACKAGES+=(-p "$package")
  BINS+=(--bin "${pair#*:}")
done

echo "FEATURES=\"$FEATURES\""
echo "RUSTFLAGS=\"$RUSTFLAGS\""
rust-zig-builder cargo zigbuild --manifest-path=./Cargo.toml "${BUILD_FLAGS[@]}" --features="$FEATURES" --locked "${PACKAGES[@]}" "${BINS[@]}" --target="$TARGET"
if [ "$(ls -nd "target/$TARGET/$PROFILE/${1#*:}" | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
fi
