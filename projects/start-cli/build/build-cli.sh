#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

set -e

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

ARCH=${ARCH:-$(uname -m)}
if [ "$ARCH" = arm64 ]; then
  ARCH=aarch64
fi
RUST_ARCH=${ARCH/riscv64/riscv64gc}

if [ -z "${TARGET:-}" ]; then
  case "${KERNEL_NAME:-$(uname -s)}" in
    Linux) TARGET="$RUST_ARCH-unknown-linux-musl" ;;
    Darwin) TARGET="$RUST_ARCH-apple-darwin" ;;
    *)
      >&2 echo "unknown kernel ${KERNEL_NAME:-$(uname -s)}"
      exit 1
      ;;
  esac
fi

TARGET=$TARGET ./build/build-rust-bins.sh start-cli:start-cli

if [ "$INSTALL" = "true" ]; then
  cp "target/$TARGET/${PROFILE:-release}/start-cli" ~/.cargo/bin/start-cli
fi
