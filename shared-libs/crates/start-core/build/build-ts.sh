#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."

source ./shared-libs/crates/start-core/build/generator-env.sh

rust-zig-builder cargo run --manifest-path=./Cargo.toml -p start-core --example generate_bindings "${BUILD_FLAGS[@]}" --features "ts,$FEATURES" --locked -- shared-libs/crates/start-core/bindings
if [ "$(ls -nd "shared-libs/crates/start-core/bindings" | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID shared-libs/crates/start-core/bindings && chown -R $UID:$UID  /usr/local/cargo"
fi
