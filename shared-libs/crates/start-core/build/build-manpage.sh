#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."

source ./shared-libs/crates/start-core/build/generator-env.sh

rust-zig-builder cargo test --manifest-path=./Cargo.toml -p start-core --lib "${BUILD_FLAGS[@]}" --features "test,$FEATURES" --locked 'export_manpage_'
# Each product's pages are generated into its own project dir (see export_manpage_* tests).
MAN_DIRS="projects/start-cli/man projects/start-registry/man projects/start-tunnel/man projects/start-os/man"
if [ "$(ls -nd projects/start-cli/man | awk '{ print $3 }')" != "$UID" ]; then
  rust-zig-builder sh -c "chown -R $UID:$UID target $MAN_DIRS /usr/local/cargo"
fi