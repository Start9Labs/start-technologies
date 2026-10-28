# syntax=docker/dockerfile:1
ARG UPSTREAM_IMAGE=ghcr.io/rust-cross/cargo-zigbuild@sha256:eb5578cfc06e070a09deb7b5852474bd128a4e561f9f123261b5ed2405e478b1
ARG ALPINE_IMAGE=alpine@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b

FROM --platform=linux/amd64 ${ALPINE_IMAGE} AS sysroot_amd64
RUN apk add --no-cache fuse3 fuse3-dev fuse3-static musl-dev pkgconfig

FROM --platform=linux/arm64 ${ALPINE_IMAGE} AS sysroot_arm64
RUN apk add --no-cache fuse3 fuse3-dev fuse3-static musl-dev pkgconfig

FROM --platform=linux/riscv64 ${ALPINE_IMAGE} AS sysroot_riscv64
RUN apk add --no-cache fuse3 fuse3-dev fuse3-static musl-dev pkgconfig

FROM ${UPSTREAM_IMAGE} AS toolchain
ARG RUST_VERSION=1.97.1
ENV RUST_VERSION=${RUST_VERSION}
RUN previous=$(rustup show active-toolchain | awk '{print $1}') && \
    rustup toolchain install "$RUST_VERSION" --profile minimal --component rustfmt,rust-src && \
    rustup default "$RUST_VERSION" && \
    rustup target add \
        x86_64-unknown-linux-gnu x86_64-unknown-linux-musl \
        aarch64-unknown-linux-gnu aarch64-unknown-linux-musl \
        riscv64gc-unknown-linux-gnu riscv64gc-unknown-linux-musl \
        arm-unknown-linux-gnueabihf arm-unknown-linux-musleabihf \
        x86_64-apple-darwin aarch64-apple-darwin \
        x86_64-pc-windows-gnu aarch64-pc-windows-gnullvm && \
    rustup toolchain uninstall "$previous"
RUN apt-get update && \
    apt-get install -y --no-install-recommends cmake lld rsync && \
    rm -rf /var/lib/apt/lists/*
ENV CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C target-cpu=generic-rv64 -C target-feature=+v,+zicsr,+zifencei,+zicntr,+zihpm,+ziccif,+ziccamoa,+zicclsm,+ziccrse,+za64rs,+zihintpause,+zic64b,+zicbom,+zicbop,+zicboz,+zba,+zbb,+zbs,+zfhmin,+zkt,+zvfhmin,+zvbb,+zvkt,+zihintntl,+zicond,+zimop,+zcmop,+zcb,+zfa,+zawrs,+supm"

FROM toolchain AS builder
WORKDIR /build/cargo-zigbuild
ADD --checksum=sha256:62239c71c4cb36c47fe000efebfcf6fe63dc7c0feb2346434e950b124220b2e8 https://codeload.github.com/rust-cross/cargo-zigbuild/tar.gz/2ffd80658c96b5ff971085dd509e58e5b441f1c8 /tmp/cargo-zigbuild.tar.gz
RUN tar -xzf /tmp/cargo-zigbuild.tar.gz --strip-components=1 -C .
COPY rva23.patch /tmp/rva23.patch
RUN patch --fuzz=0 -p1 < /tmp/rva23.patch
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/cargo-zigbuild/target \
    cargo test --locked --lib && \
    cargo build --locked --release --bin cargo-zigbuild && \
    install -m 755 target/release/cargo-zigbuild /usr/local/bin/cargo-zigbuild-start9

WORKDIR /build/bindgen-cli
ADD --checksum=sha256:8a408c0fcb20bf7bd4ceaf4bf990e223e3543a04b84d2394f3edeee29a0e87e2 https://static.crates.io/crates/bindgen-cli/bindgen-cli-0.72.1.crate /tmp/bindgen-cli.crate
RUN tar -xzf /tmp/bindgen-cli.crate --strip-components=1 -C .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/bindgen-cli/target \
    cargo build --locked --release --bin bindgen && \
    install -m 755 target/release/bindgen /usr/local/bin/bindgen-start9

FROM toolchain
ARG SOURCE_REVISION
LABEL org.opencontainers.image.source="https://github.com/Start9Labs/start-technologies" \
    org.opencontainers.image.revision="${SOURCE_REVISION}" \
    org.opencontainers.image.description="Start9 Rust cross-compilation environment"
COPY --from=builder /usr/local/bin/cargo-zigbuild-start9 /usr/local/cargo/bin/cargo-zigbuild
COPY --from=builder /build/cargo-zigbuild/LICENSE /usr/local/share/licenses/cargo-zigbuild/LICENSE
COPY --from=builder /usr/local/bin/bindgen-start9 /usr/local/cargo/bin/bindgen
COPY --from=builder /build/bindgen-cli/LICENSE /usr/local/share/licenses/bindgen-cli/LICENSE
COPY --from=sysroot_amd64 / /opt/sysroot/x86_64/
COPY --from=sysroot_arm64 / /opt/sysroot/aarch64/
COPY --from=sysroot_riscv64 / /opt/sysroot/riscv64/
COPY verify.sh /usr/local/libexec/start9/verify-rust-builder
