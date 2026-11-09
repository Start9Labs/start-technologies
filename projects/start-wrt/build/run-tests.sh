#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

source ./builder-alias.sh

set -ea
shopt -s expand_aliases
ARCH=$(native_arch)

PROFILE=${PROFILE:-release}
if [ "${PROFILE}" = "release" ]; then
	BUILD_FLAGS="--release"
else
	if [ "$PROFILE" != "debug" ]; then
		>&2 echo "Unknown profile $PROFILE: falling back to debug..."
		PROFILE=debug
	fi
fi

cd ../../..

rust-zig-builder cargo test --manifest-path=./Cargo.toml $BUILD_FLAGS -p startwrt-core -p uciedit -p uciedit_macros --locked --lib
rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
