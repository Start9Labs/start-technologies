#!/bin/bash

source ./build/builder-alias.sh
source ./build/rust-build-profile.sh

set -ea
shopt -s expand_aliases

set_generated_profile

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
