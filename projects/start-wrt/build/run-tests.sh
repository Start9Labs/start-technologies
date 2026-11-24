#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"

source ./builder-alias.sh
source ../../../build/rust-build-profile.sh

set -ea
shopt -s expand_aliases
ARCH=$(native_arch)

set_test_profile

cd ../../..

rust-zig-builder cargo test --manifest-path=./Cargo.toml "${BUILD_FLAGS[@]}" -p startwrt-core -p uciedit -p uciedit_macros -p startwrt-dnsfwd --locked --lib
rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
