#!/bin/bash

USE_TTY=
if tty -s; then
  USE_TTY="-it"
fi

alias 'rust-zig-builder'='docker run '"$USE_TTY"' --rm \
  -e "RUSTFLAGS=$RUSTFLAGS" \
  -e "CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS" \
  -e "CC_riscv64gc_unknown_linux_musl=$CC_riscv64gc_unknown_linux_musl" \
  -e "CXX_riscv64gc_unknown_linux_musl=$CXX_riscv64gc_unknown_linux_musl" \
  -e "CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER=$CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER" \
  -e "PKG_CONFIG_SYSROOT_DIR=/opt/sysroot/$ARCH" \
  -e PKG_CONFIG_PATH="" \
  -e PKG_CONFIG_LIBDIR="/opt/sysroot/$ARCH/usr/lib/pkgconfig" \
  -e STARTWRT_GIT_HASH \
  -e SCCACHE_GHA_ENABLED \
  -e SCCACHE_GHA_VERSION \
  -e ACTIONS_RESULTS_URL \
  -e ACTIONS_RUNTIME_TOKEN \
  -v "$HOME/.cargo/registry":/usr/local/cargo/registry \
  -v "$HOME/.cargo/git":/usr/local/cargo/git \
  -v "$HOME/.cache/sccache":/root/.cache/sccache \
  -v "$HOME/.cache/cargo-zigbuild:/root/.cache/cargo-zigbuild" \
  -v "$(pwd)":/workdir \
  -w /workdir \
  start9/cargo-zigbuild'
