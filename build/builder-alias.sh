#!/bin/bash

RUST_BUILDER_IMAGE=$(RUST_BUILDER_IMAGE="${RUST_BUILDER_IMAGE:-}" "$(dirname "${BASH_SOURCE[0]}")/rust-builder/image.sh")

# Build flags and output directories use the same normalized profile.
set_build_profile() {
  PROFILE=${PROFILE:-release}
  BUILD_FLAGS=
  case "$PROFILE" in
    release) BUILD_FLAGS=--release ;;
    debug) ;;
    *)
      >&2 echo "Unknown profile $PROFILE: falling back to debug..."
      PROFILE=debug
      ;;
  esac
}

USE_TTY=
if tty -s; then
  USE_TTY="-it"
fi

alias 'rust-zig-builder'='docker run '"$USE_TTY"' --rm -e "RUSTFLAGS=$RUSTFLAGS" -e "PKG_CONFIG_SYSROOT_DIR=/opt/sysroot/$ARCH" -e PKG_CONFIG_PATH="" -e PKG_CONFIG_LIBDIR="/opt/sysroot/$ARCH/usr/lib/pkgconfig" -e SCCACHE_GHA_ENABLED -e SCCACHE_GHA_VERSION -e ACTIONS_RESULTS_URL -e ACTIONS_RUNTIME_TOKEN -v "$HOME/.cargo/registry":/usr/local/cargo/registry -v "$HOME/.cargo/git":/usr/local/cargo/git -v "$HOME/.cache/sccache":/root/.cache/sccache -v "$HOME/.cache/cargo-zigbuild:/root/.cache/cargo-zigbuild" -v "$(pwd)":/workdir -w /workdir "$RUST_BUILDER_IMAGE"'