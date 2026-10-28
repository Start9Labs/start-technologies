#!/bin/bash
set -euo pipefail

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

cargo-zigbuild --version | grep -Fx 'cargo-zigbuild 0.24.0'
zig version
rustc --version
bindgen --version
rustfmt --version
cmake --version
ld.lld --version
rsync --version
for arch in x86_64 aarch64 riscv64; do
  PKG_CONFIG_LIBDIR="/opt/sysroot/$arch/usr/lib/pkgconfig" \
    PKG_CONFIG_SYSROOT_DIR="/opt/sysroot/$arch" pkg-config --exists fuse3
done

printf 'int answer(void);\n' > "$scratch/probe.h"
bindgen "$scratch/probe.h" --output "$scratch/bindings.rs"
grep -F 'pub fn answer(' "$scratch/bindings.rs"

printf '#define FUSE_USE_VERSION 31\n#include <fuse_lowlevel.h>\nint main(void) { return fuse_version() < 300; }\n' > "$scratch/fuse.c"
host_arch=$(uname -m)
export PKG_CONFIG_LIBDIR="/opt/sysroot/$host_arch/usr/lib/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="/opt/sysroot/$host_arch"
read -ra fuse_flags <<< "$(pkg-config --cflags --libs --static fuse3)"
cargo-zigbuild zig cc -- -target "$host_arch-linux-musl" -static \
  "$scratch/fuse.c" "${fuse_flags[@]}" -o "$scratch/fuse"
"$scratch/fuse"
unset PKG_CONFIG_LIBDIR PKG_CONFIG_SYSROOT_DIR

printf 'extern void *objc_getClass(const char *);\nint main(void) { return objc_getClass("NSObject") == 0; }\n' > "$scratch/main.c"
for target in aarch64-macos-none x86_64-macos-none; do
  for mode in direct response; do
    bin="$scratch/$target-$mode"
    args=("$scratch/main.c" -lobjc -lobjc -o "$bin")
    if [ "$mode" = response ]; then
      printf '%s\n' "${args[@]}" > "$scratch/linker-arguments"
      args=("@$scratch/linker-arguments")
    fi
    cargo-zigbuild zig cc -- -target "$target" "${args[@]}"
    llvm-objdump-19 --macho --dylibs-used "$bin" > "$scratch/dylibs"
    test "$(grep -c '/usr/lib/libobjc.A.dylib ' "$scratch/dylibs")" = 1
    test -z "$(tail -n +2 "$scratch/dylibs" | awk '{print $1}' | sort | uniq -d)"
    printf '%s %s: no duplicate dylibs\n' "$target" "$mode"
  done
done

mkdir "$scratch/probe"
cd "$scratch/probe"
printf '[package]\nname = "builder-probe"\nversion = "0.1.0"\nedition = "2024"\n' > Cargo.toml
mkdir src
printf 'fn main() { println!("builder probe passed"); }\n' > src/main.rs
printf 'int answer(void) { return 42; }\n' > probe.c
printf 'cmake_minimum_required(VERSION 3.16)\nproject(builder_probe C)\nadd_library(cmake_probe OBJECT probe.c)\n' > CMakeLists.txt
printf '%s\n' \
  'use std::{env, path::PathBuf, process::Command};' \
  'fn main() {' \
  '    let target = env::var("TARGET").unwrap().replace("-", "_");' \
  '    let cc = env::var(format!("CC_{target}")).unwrap();' \
  '    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("probe.o");' \
  '    assert!(Command::new(cc).args(["-c", "probe.c", "-o"]).arg(&out).status().unwrap().success());' \
  '    let toolchain = env::var(format!("CMAKE_TOOLCHAIN_FILE_{target}")).unwrap();' \
  '    let build = out.parent().unwrap().join("cmake");' \
  '    assert!(Command::new("cmake").args(["-S", ".", "-B"]).arg(&build).arg(format!("-DCMAKE_TOOLCHAIN_FILE={toolchain}")).status().unwrap().success());' \
  '    assert!(Command::new("cmake").arg("--build").arg(build).status().unwrap().success());' \
  '}' > build.rs
cargo generate-lockfile --offline

env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  -u CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS \
  cargo zigbuild --locked --target riscv64gc-unknown-linux-musl --target-dir "$scratch/rva23"
readelf -A "$scratch"/rva23/riscv64gc-unknown-linux-musl/debug/build/builder-probe-*/out/probe.o > "$scratch/isa"
for feature in v1p0 zvbb1p0 zicond1p0 supm1p0; do
  grep -q "_$feature" "$scratch/isa"
done

env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS \
  RUSTFLAGS='-C target-cpu=sifive-u74' \
  cargo zigbuild --locked --target riscv64gc-unknown-linux-musl --target-dir "$scratch/cpu-override"
readelf -A "$scratch"/cpu-override/riscv64gc-unknown-linux-musl/debug/build/builder-probe-*/out/probe.o > "$scratch/isa"
if grep -q '_v[0-9]' "$scratch/isa"; then
  printf 'Explicit scalar CPU unexpectedly enables vector instructions\n' >&2
  exit 1
fi

host_target="$host_arch-unknown-linux-musl"
cargo zigbuild --locked --target "$host_target" --target-dir "$scratch/native"
"$scratch/native/$host_target/debug/builder-probe" | grep -Fx 'builder probe passed'
