#!/bin/bash
set -ea
shopt -s expand_aliases

cd "$(git rev-parse --show-toplevel)"
source projects/start-wrt/build/builder-alias.sh

trap 'rust-zig-builder chown -R "$UID:$UID" target /usr/local/cargo "$@"' EXIT

rust-zig-builder cargo run --profile bindings -p startwrt-core --example generate_rpc_bindings --features ts --locked -- "$1"
