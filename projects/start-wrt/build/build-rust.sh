#!/bin/bash
set -ea
shopt -s expand_aliases

# Monorepo-aware build. Run from the monorepo root (the build.mk recipe does so);
# the cargo workspace root is the monorepo root, so the docker builder mounts the
# whole monorepo as /workdir and startwrt-core's path-deps into shared-libs/ resolve.
cd "$(git rev-parse --show-toplevel)"

PROJECT_DIR=projects/start-wrt
source "$PROJECT_DIR/build/builder-alias.sh"

# Restore host ownership of Docker-written artifacts on exit. The cargo target dir
# is now the workspace-root target/ (not backend/target/), since the crates are
# members of the monorepo workspace.
trap 'rust-zig-builder sh -c "chown -R $UID:$UID target /usr/local/cargo 2>/dev/null || true"' EXIT

ARCH=${ARCH:-riscv64}
RUST_ARCH=${RUST_ARCH:-riscv64gc}
PROFILE=${PROFILE:-release}
if [ -z "$STARTWRT_GIT_HASH" ]; then
    STARTWRT_GIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
    git diff-index --quiet HEAD -- 2>/dev/null || STARTWRT_GIT_HASH="${STARTWRT_GIT_HASH}-dirty"
fi
export STARTWRT_GIT_HASH

if [ "${PROFILE}" = "release" ]; then
	BUILD_FLAGS="--release"
fi

# SpaceMiT K1 (BPI-F3) is RVA22 + V 1.0 + Zicbom/Zicbop/Zicboz — no RVA23.
# The cargo-zigbuild image bakes CARGO_TARGET_<triple>_RUSTFLAGS with an
# RVA23 profile (+zicond, +zfa, +zimop, +zcmop, +zvbb, +zvkt, etc.) that
# SIGILLs on K1. Use RUSTFLAGS (not the CARGO_TARGET_<triple> variant) —
# cargo-zigbuild reads RUSTFLAGS to select its linker wrapper, so setting it
# also makes it pick the clean generic_rv64 wrapper instead of the RVA23 one.
if [ "$RUST_ARCH" = "riscv64gc" ]; then
	# Clear the image's RVA23 default so our RUSTFLAGS takes effect
	# (CARGO_TARGET_<triple>_RUSTFLAGS outranks RUSTFLAGS when both are set).
	export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS=""
	# Deliberately NO `+v`. K1 implements V 1.0 but traps on misaligned
	# vector-element accesses, and the Bianbu 6.6 kernel does not emulate
	# them — so any V instruction LLVM's auto-vectoriser emits (blake3,
	# memcpy, TLS record assembly) surfaces as a userspace SIGBUS/BUS_ADRALN
	# with no Rust panic. Keep V off globally; re-enable per-audited-crate
	# only. build/verify-isa.sh fails the build if V instructions appear.
	export RUSTFLAGS="-C target-cpu=generic-rv64 -C target-feature=+m,+a,+f,+d,+c,+zicsr,+zifencei,+zicntr,+zihpm,+ziccif,+ziccamoa,+zicclsm,+ziccrse,+za64rs,+zihintpause,+zic64b,+zicbom,+zicbop,+zicboz,+zba,+zbb,+zbs,+zkt,+zihintntl,+zawrs"
	# C deps (aws-lc-sys, ring, openssl-sys, zstd-sys) go through our
	# K1-pinned zigcc wrapper, which bakes -mcpu into its zig cc invocation.
	# Do NOT also set CFLAGS_<target>: aws-lc-sys's jitter-entropy sub-build
	# appends `-Wp,-U_FORTIFY_SOURCE` only when a user CFLAGS is present, and
	# `zig cc` rejects `-Wp,` passthrough flags. Paths are container-relative
	# (/workdir is the mounted monorepo root).
	export CC_riscv64gc_unknown_linux_musl=/workdir/$PROJECT_DIR/build/zigcc-k1.sh
	export CXX_riscv64gc_unknown_linux_musl=/workdir/$PROJECT_DIR/build/zigcxx-k1.sh
	export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER=/workdir/$PROJECT_DIR/build/zigcc-k1.sh
fi

# start-core's build.rs reads build/env/{GIT_HASH,ENVIRONMENT}.txt from the
# monorepo root; generate them first so the in-container build finds them.
./build/env/check-git-hash.sh >/dev/null
ENVIRONMENT="${ENVIRONMENT:-}" ./build/env/check-environment.sh >/dev/null

echo "==> Building Rust binaries (arch=$RUST_ARCH, profile=$PROFILE)..."
rust-zig-builder cargo zigbuild \
    --manifest-path=./$PROJECT_DIR/backend/ctrl/Cargo.toml \
    $BUILD_FLAGS \
    --locked \
    --target=${RUST_ARCH}-unknown-linux-musl

# Verify the binary doesn't use any RVA23-only instructions that would
# SIGILL on the SpaceMiT K1 (BPI-F3) target.
if [ "$RUST_ARCH" = "riscv64gc" ]; then
    bash "$PROJECT_DIR/build/verify-isa.sh" "target/${RUST_ARCH}-unknown-linux-musl/${PROFILE}/startwrt"
fi

echo "==> Rust build complete."
