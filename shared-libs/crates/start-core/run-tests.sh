#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

source ./shared-libs/crates/start-core/build/test-env.sh

rust-zig-builder cargo test "${TEST_ARGS[@]}" -- --skip export_
rust-zig-builder sh -c "PATH=\"\$PWD/shared-libs/crates/start-core/testdata/backup-conversion:\$PATH\" cargo test ${TEST_ARGS[*]} backup::target::conversion::tests::worker_ -- --ignored"
rust-zig-builder sh -c "chown -R $UID:$UID target && chown -R $UID:$UID /usr/local/cargo"
