#!/bin/bash

RUST_BUILDER_IMAGE=$(RUST_BUILDER_IMAGE="${RUST_BUILDER_IMAGE:-}" "$(dirname "${BASH_SOURCE[0]}")/rust-builder/image.sh")

USE_TTY=
if tty -s; then
  USE_TTY="-it"
fi

# ZIG_LIBC links musl targets against the sysroot's musl. Zig's bundled malloc never returns freed memory.
alias 'rust-zig-builder'='docker run '"$USE_TTY"' --rm -e "RUSTFLAGS=$RUSTFLAGS" -e "ZIG_LIBC=/workdir/build/musl-libc/$ARCH.txt" -e "PKG_CONFIG_SYSROOT_DIR=/opt/sysroot/$ARCH" -e PKG_CONFIG_PATH="" -e PKG_CONFIG_LIBDIR="/opt/sysroot/$ARCH/usr/lib/pkgconfig" -e SCCACHE_GHA_ENABLED -e SCCACHE_GHA_VERSION -e ACTIONS_RESULTS_URL -e ACTIONS_RUNTIME_TOKEN -v "$HOME/.cargo/registry":/usr/local/cargo/registry -v "$HOME/.cargo/git":/usr/local/cargo/git -v "$HOME/.cache/sccache":/root/.cache/sccache -v "$HOME/.cache/cargo-zigbuild:/root/.cache/cargo-zigbuild" -v "$(pwd)":/workdir -w /workdir "$RUST_BUILDER_IMAGE"'