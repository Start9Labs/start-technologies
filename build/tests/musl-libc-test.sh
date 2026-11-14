#!/bin/bash
# Builds a probe through the shared builder alias for each musl architecture and
# checks which allocator it links. Requires Docker and the pinned builder image.
set -euo pipefail
shopt -s expand_aliases

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
RUSTFLAGS=${RUSTFLAGS:-}
ARCH=${ARCH:-}
source ./build/builder-alias.sh

probe=target/musl-libc-test
trap 'rust-zig-builder sh -c "chown -R $UID:$UID target /usr/local/cargo 2>/dev/null || true"' EXIT
rm -rf "$probe"
mkdir -p "$probe/src"
cat > "$probe/Cargo.toml" <<'EOF'
[package]
name = "musl-libc-probe"
version = "0.0.0"
edition = "2021"

[workspace]

[profile.release]
strip = "none"
EOF
printf 'fn main() {\n    println!("{}", std::hint::black_box(vec![0u8; 64]).len());\n}\n' > "$probe/src/main.rs"

for ARCH in x86_64 aarch64 riscv64; do
  target="${ARCH/riscv64/riscv64gc}-unknown-linux-musl"
  rust-zig-builder cargo zigbuild --manifest-path="$probe/Cargo.toml" --release --target="$target" --target-dir="$probe/target"
  symbols=$(readelf --wide --symbols "$probe/target/$target/release/musl-libc-probe")
  for symbol in __libc_malloc_impl alloc_slot; do
    if ! grep -q " $symbol\$" <<< "$symbols"; then
      printf '%s does not link musl mallocng (%s missing)\n' "$target" "$symbol" >&2
      exit 1
    fi
  done
  if grep -q 'SmpAllocator' <<< "$symbols"; then
    printf '%s links Zig SmpAllocator\n' "$target" >&2
    exit 1
  fi
  printf '%s links musl mallocng\n' "$target"
done
