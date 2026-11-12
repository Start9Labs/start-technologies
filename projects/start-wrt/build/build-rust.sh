#!/bin/bash
set -ea
shopt -s expand_aliases

cd "$(git rev-parse --show-toplevel)"

PROJECT_DIR=projects/start-wrt
source "$PROJECT_DIR/build/builder-alias.sh"

trap 'rust-zig-builder sh -c "chown -R $UID:$UID target /usr/local/cargo 2>/dev/null || true"' EXIT

ARCH=${ARCH:-riscv64}
RUST_ARCH=${RUST_ARCH:-riscv64gc}
PROFILE=${PROFILE:-release}
if [ -z "$STARTWRT_GIT_HASH" ]; then
    # The UI compares this stamp to detect stale cached bundles.
    "$PROJECT_DIR/build/env/check-git-hash.sh" >/dev/null || true
    STARTWRT_GIT_HASH=$(cat "$PROJECT_DIR/build/env/GIT_HASH.txt" 2>/dev/null || echo unknown)
fi
export STARTWRT_GIT_HASH

if [ "${PROFILE}" = "release" ]; then
	BUILD_FLAGS="--release"
fi

if [ "$RUST_ARCH" = "riscv64gc" ]; then
	# K1 traps on misaligned vector accesses; its kernel cannot emulate them.
	export RUSTFLAGS="-C target-cpu=generic-rv64 -C target-feature=+m,+a,+f,+d,+c,+zicsr,+zifencei,+zicntr,+zihpm,+ziccif,+ziccamoa,+zicclsm,+ziccrse,+za64rs,+zihintpause,+zic64b,+zicbom,+zicbop,+zicboz,+zba,+zbb,+zbs,+zkt,+zihintntl,+zawrs"
	# User CFLAGS make aws-lc-sys add -Wp passthrough flags Zig rejects.
	export CC_riscv64gc_unknown_linux_musl=/workdir/$PROJECT_DIR/build/zigcc-k1.sh
	export CXX_riscv64gc_unknown_linux_musl=/workdir/$PROJECT_DIR/build/zigcxx-k1.sh
	export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER=/workdir/$PROJECT_DIR/build/zigcc-k1.sh
fi

./build/env/check-git-hash.sh >/dev/null
ENVIRONMENT="${ENVIRONMENT:-}" ./build/env/check-environment.sh >/dev/null

echo "==> Building Rust binaries (arch=$RUST_ARCH, profile=$PROFILE)..."
rust-zig-builder cargo zigbuild \
    --manifest-path=./$PROJECT_DIR/backend/ctrl/Cargo.toml \
    $BUILD_FLAGS \
    --locked \
    --target=${RUST_ARCH}-unknown-linux-musl

if [ "$RUST_ARCH" = "riscv64gc" ]; then
    bash "$PROJECT_DIR/build/verify-isa.sh" "target/${RUST_ARCH}-unknown-linux-musl/${PROFILE}/startwrt"
fi

echo "==> Rust build complete."
