# Rust builder image

The [Rust Builder workflow](../../.github/workflows/rust-builder.yaml) builds and verifies native amd64 and arm64 images. After both architectures pass, master builds publish a combined manifest to the GHCR repository named in `repository`, tagged `sha-<full-commit>` and `latest`. Pull requests build and verify without publishing. A manual dispatch on master runs the same publishing pipeline.

The Dockerfile derives from the digest-pinned upstream `ghcr.io/rust-cross/cargo-zigbuild:0.24.0` image. It retains upstream's Zig 0.16.0, Clang and macOS SDK, installs Rust 1.97.1 with the supported targets, and adds CMake, lld and rsync. Three digest-pinned Alpine 3.24.1 stages supply musl, FUSE development/static libraries and pkg-config metadata at the paths the build scripts consume. Bindgen 0.72.1 is built from its checksum-verified published crate.

A build stage downloads checksum-verified upstream cargo-zigbuild 0.24.0 source, applies `rva23.patch` and `zig-libc.patch`, runs its library tests, and compiles the replacement executable. The patch preserves Start9's RVA23 default for `riscv64gc` C/C++ compilation using Zig's named profile. The entrypoint supplies target-specific Rust features while retaining Rust's built-in `generic-rv64` CPU default. Leaving the CPU implicit lets cargo-zigbuild select its RVA23 C/C++ profile instead of replacing it with Zig's extension-free `generic_rv64` CPU. Explicit Rust flags and CPU overrides retain their precedence. CMake toolchain filenames reuse the CPU-specific compiler wrapper identity, keeping profiles isolated in shared caches. The upstream release supplies the Apple dylib deduplication and deployment-target handling.

The image does not include the legacy GNU cross-compiler tree or its CMake overrides. Monorepo callers use Zig and cargo-zigbuild's generated CMake toolchains; no caller references those legacy paths.

The final image contains the replacement executable and its verification script, not the downloaded source or intermediate Rust build artifacts.

## RISC-V profile

RVA23 is enabled by default for `riscv64gc`. Pass `docker run -e DISABLE_RVA23=1 …` to disable both the entrypoint's default Rust flags and cargo-zigbuild's default C/C++ profile. Cargo-zigbuild then uses upstream's baseline `riscv64gc` C/C++ profile, including its generated CMake toolchains. Explicit caller-supplied Rust flags, compiler wrappers and CPU overrides still apply. The explicit `riscv64a23` target retains its required profile.

The entrypoint executes the requested command directly. Replacing it with Docker's `--entrypoint` bypasses the default Rust flags; the cargo-zigbuild profile switch still applies.

## Musl libc

`zig-libc.patch` makes `cargo-zigbuild zig cc` take a target's libc paths file from `ZIG_LIBC_<zig target>`, with `-` and `.` replaced by `_`. The image sets it for `x86_64-linux-musl`, `aarch64-linux-musl` and `riscv64-linux-musl`, pointing at files beside the Alpine sysroots. Those targets statically link Alpine's musl and its mallocng allocator rather than Zig 0.16's bundled libc, whose malloc never returns freed small allocations to the operating system. Other targets, including Apple's, keep Zig's libc. Cargo builds reach the patch through cargo-zigbuild's compiler wrappers, and StartWRT's K1 wrapper calls it with `-target riscv64-linux-musl`. `startbox`, `registrybox` and `tunnelbox` declare mimalloc as Rust's global allocator, so only their C and libc allocations use mallocng.

Cargo fingerprints compiler wrapper paths, not the libc or Zig release behind them. The image sets `CARGO_ZIGBUILD_CACHE_DIR` to a directory named after its source revision, so a new image moves the wrappers, and the first build after a reference change recompiles C dependencies and relinks. The image check warms a probe with Zig's libc in another cache directory, rebuilds it, and checks that each musl target links mallocng.

## Consumers

`image.sh` resolves the published image from `repository` and `reference`, or returns a `RUST_BUILDER_IMAGE` override. Shared compiler commands, StartWRT's compiler/bindings/test/disassembly commands, and the formatter use that selection. StartWRT's shared builder alias disables the default RVA23 profile; native bindings generation uses the image's pinned Rust compiler, while router cross-compilation retains its scalar K1 flags and compiler wrappers. Rust binary Make prerequisites track the selection files and the shared builder alias, so a change to either invalidates existing binaries.

The formatter and core-test Dockerfiles receive the base as a build argument. Docker's build cache refreshes derived images when their base or recipe changes. Core tests keep their separate `RUST_BUILDER_IMAGE` contract: a custom value selects a prebuilt test image; the default test image is built over the pinned builder with Node and SquashFS tools.

Run `make rust-builder-selection-test` for shell checks of the entrypoint's profile defaults, opt-out, explicit flags and command forwarding, plus Docker-mocked checks of image selection, containerized bindings generation and failure cleanup, overrides, derived-image build arguments, and the K1 disassembler's native override.

## Local build

From the monorepo root:

```sh
make rust-builder
make rust-builder-check
```

`RUST_BUILDER_TEST_IMAGE` overrides the local image tag. Building all three sysroots requires QEMU/binfmt for foreign architectures; CI configures it through the shared build-setup action. The image check verifies the default shell with no explicit container command, bindgen output, pkg-config discovery for all three FUSE sysroots, native static FUSE linking, macOS dylib deduplication on both architectures and both argument paths, RISC-V ISA defaults and CPU overrides through generated compiler wrappers, default Rust/C/CMake ISA attributes, the RVA23 opt-out in Rust/C/CMake artifacts, preservation of cached toolchains across profile switches, CMake compilation through upstream-generated toolchains, and a native Linux-musl Rust executable.

## Updates

Update the upstream image digest and source URL/checksum together, then re-evaluate the patch against that release. Rust, bindgen and the Alpine sysroot stages have separate explicit version pins. The Alpine pins also select the musl release that musl binaries link.

GHCR associates the package with this repository through its OCI source label. Its package visibility must be public before unauthenticated developer builds can pull it.
