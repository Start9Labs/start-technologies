# rustfmt for the monorepo, pinned for byte-for-byte reproducible output.
# Based on start9/cargo-zigbuild (the same image the Rust build runs in — it
# already ships rustup + cargo), so we only add the pinned nightly + rustfmt
# component. rustfmt needs nightly because our rustfmt.toml uses nightly-only
# options (group_imports, imports_granularity). Only rustfmt needs the
# container; prettier and taplo are pinned via npm and run natively.
# Bump RUSTFMT_TOOLCHAIN to upgrade.
FROM start9/cargo-zigbuild

ARG RUSTFMT_TOOLCHAIN=nightly-2026-05-28

RUN rustup toolchain install "$RUSTFMT_TOOLCHAIN" --profile minimal --component rustfmt \
    && cargo "+$RUSTFMT_TOOLCHAIN" fmt --version
