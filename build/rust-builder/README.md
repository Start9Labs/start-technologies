# Rust builder image

The [Rust Builder workflow](../../.github/workflows/rust-builder.yaml) builds and verifies native amd64 and arm64 images. After both architectures pass, master builds publish a combined manifest to the GHCR repository named in `repository`, tagged `sha-<full-commit>` and `latest`. Pull requests build and verify without publishing. A manual dispatch on master runs the same publishing pipeline.

The Dockerfile derives from the digest-pinned upstream `ghcr.io/rust-cross/cargo-zigbuild:0.24.0` image. It retains upstream's Zig 0.16.0, Clang and macOS SDK, installs Rust 1.97.1 with the supported targets, and adds CMake, lld and rsync. Three digest-pinned Alpine 3.24.1 stages supply FUSE development/static libraries and pkg-config metadata at the paths the build scripts consume. Bindgen 0.72.1 is built from its checksum-verified published crate.

A build stage downloads checksum-verified upstream cargo-zigbuild 0.24.0 source, applies `rva23.patch`, runs its library tests, and compiles the replacement executable. The patch preserves Start9's RVA23 default for `riscv64gc` C/C++ compilation using Zig's named profile. Explicit CPU overrides and the image's target-specific Rust flags retain their previous behavior. The upstream release supplies the Apple dylib deduplication and deployment-target handling.

The image does not include the legacy GNU cross-compiler tree or its CMake overrides. Monorepo callers use Zig and cargo-zigbuild's generated CMake toolchains; no caller references those legacy paths.

The final image contains the replacement executable and its verification script, not the downloaded source or intermediate Rust build artifacts.

## Consumers

`image.sh` resolves the published image from `repository` and `reference`, or returns a `RUST_BUILDER_IMAGE` override. Shared compiler commands, StartWRT's compiler/test/disassembly commands, and the formatter use that selection. Rust binary Make prerequisites track the selection files, so a pin change invalidates existing binaries.

The formatter and core-test Dockerfiles receive the base as a build argument. Docker's build cache refreshes derived images when their base or recipe changes. Core tests keep their separate `RUST_BUILDER_IMAGE` contract: a custom value selects a prebuilt test image; the default test image is built over the pinned builder with Node and SquashFS tools.

Run `make rust-builder-selection-test` for Docker-mocked checks of image selection, overrides, derived-image build arguments, and the K1 disassembler's native override.

## Local build

From the monorepo root:

```sh
make rust-builder
make rust-builder-check
```

`RUST_BUILDER_TEST_IMAGE` overrides the local image tag. Building all three sysroots requires QEMU/binfmt for foreign architectures; CI configures it through the shared build-setup action. The image check verifies bindgen output, pkg-config discovery for all three FUSE sysroots, native static FUSE linking, macOS dylib deduplication on both architectures and both argument paths, RISC-V ISA defaults and CPU overrides through generated compiler wrappers, CMake compilation through upstream-generated toolchains, and a native Linux-musl Rust executable.

## Updates

Update the upstream image digest and source URL/checksum together, then re-evaluate the patch against that release. Rust, bindgen and the Alpine sysroot stages have separate explicit version pins.

GHCR associates the package with this repository through its OCI source label. Its package visibility must be public before unauthenticated developer builds can pull it.
