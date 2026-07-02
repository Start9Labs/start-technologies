#!/bin/bash

# Run start-wrt's Rust unit tests inside the start9/cargo-zigbuild container,
# mirroring shared-libs/crates/start-core/run-tests.sh. Package-scoped to the
# three start-wrt crates so it never tries to compile other workspace members
# (notably startos-backup-fs -> fuser, which needs FUSE libs only present in the
# container's sysroot). Host-target tests (`cargo test`, not zigbuild).

cd "$(dirname "${BASH_SOURCE[0]}")"

source ./builder-alias.sh

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

# Mount the monorepo root (the cargo workspace root) so startwrt-core's path-deps
# into shared-libs/ resolve; the builder alias mounts $(pwd) as /workdir.
cd ../../..

# No --features: start-wrt crates define none, and start-core's `test` feature is
# for start-core's own tests, not start-wrt's (the one deliberate deviation from
# start-core/run-tests.sh).
rust-zig-builder cargo test --manifest-path=./Cargo.toml $BUILD_FLAGS -p startwrt-core -p uciedit -p uciedit_macros --locked --lib
rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
