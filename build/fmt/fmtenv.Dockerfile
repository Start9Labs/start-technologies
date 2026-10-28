ARG RUST_BUILDER_IMAGE
FROM ${RUST_BUILDER_IMAGE}

ARG RUSTFMT_TOOLCHAIN=nightly-2026-05-28

RUN rustup toolchain install "$RUSTFMT_TOOLCHAIN" --profile minimal --component rustfmt \
    && cargo "+$RUSTFMT_TOOLCHAIN" fmt --version
