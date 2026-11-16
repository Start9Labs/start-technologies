#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

source ./builder-alias.sh

set -ea
shopt -s expand_aliases
ARCH=$(native_arch)

PROFILE=${PROFILE:-unit-test}
case "$PROFILE" in
	release) BUILD_FLAGS="--release" ;;
	unit-test) BUILD_FLAGS="--profile unit-test" ;;
	debug) BUILD_FLAGS="" ;;
	*)
		>&2 echo "Unknown profile $PROFILE: falling back to debug..."
		PROFILE=debug
		BUILD_FLAGS=""
		;;
esac

cd ../../..

rust-zig-builder cargo test --manifest-path=./Cargo.toml $BUILD_FLAGS -p startwrt-core -p uciedit -p uciedit_macros --locked --lib
rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
