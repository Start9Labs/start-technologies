#!/bin/bash

RUST_BUILDER_IMAGE=$(RUST_BUILDER_IMAGE="${RUST_BUILDER_IMAGE:-}" "$(dirname "${BASH_SOURCE[0]}")/rust-builder/image.sh")

# Cargo must not reuse native code or link outputs across libc selections.
RUST_ZIG_BUILDER_CACHE_KEY=$(
  cd "$(dirname "${BASH_SOURCE[0]}")" || exit
  {
    printf '%s\n' "$RUST_BUILDER_IMAGE"
    git hash-object musl-libc/*.txt
  } | git hash-object --stdin
)

USE_TTY=
if tty -s; then
  USE_TTY="-it"
fi

# ZIG_LIBC links musl targets against the sysroot's musl. Zig's bundled malloc never returns freed memory.
alias 'rust-zig-builder'='docker run '"$USE_TTY"' --rm -e "RUSTFLAGS=$RUSTFLAGS" -e "ZIG_LIBC=/workdir/build/musl-libc/$ARCH.txt" -e "CARGO_ZIGBUILD_CACHE_DIR=/root/.cache/cargo-zigbuild/sysroot-$RUST_ZIG_BUILDER_CACHE_KEY" -e "PKG_CONFIG_SYSROOT_DIR=/opt/sysroot/$ARCH" -e PKG_CONFIG_PATH="" -e PKG_CONFIG_LIBDIR="/opt/sysroot/$ARCH/usr/lib/pkgconfig" -e SCCACHE_GHA_ENABLED -e SCCACHE_GHA_VERSION -e ACTIONS_RESULTS_URL -e ACTIONS_RUNTIME_TOKEN -v "$HOME/.cargo/registry":/usr/local/cargo/registry -v "$HOME/.cargo/git":/usr/local/cargo/git -v "$HOME/.cache/sccache":/root/.cache/sccache -v "$HOME/.cache/cargo-zigbuild:/root/.cache/cargo-zigbuild" -v "$(pwd)":/workdir -w /workdir "$RUST_BUILDER_IMAGE"'