#!/bin/bash

source ./build/builder-alias.sh

set -ea
shopt -s expand_aliases

PROFILE=${PROFILE:-generated}
case "$PROFILE" in
  debug) PROFILE=dev ;;
  dev|release|generated|bindings) ;;
  *)
    >&2 echo "Unknown profile $PROFILE: falling back to debug..."
    PROFILE=dev
    ;;
esac
BUILD_FLAGS=(--profile "$PROFILE")

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
