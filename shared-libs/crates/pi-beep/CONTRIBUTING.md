# Contributing to pi-beep

A single-file Raspberry Pi PWM `beep` reimplementation at `shared-libs/crates/pi-beep`. For general
environment setup, cloning, and the monorepo build system, see the repo-root
[CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Documentation

This crate's docs split across four files:

- `README.txt` — what this is and CLI usage (plaintext; not Markdown).
- `ARCHITECTURE.md` — how it's built and how it works.
- `CONTRIBUTING.md` — this file; how to contribute.
- `AGENTS.md` — AI/dev operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import; don't edit it).

## Prerequisites

- [Rust](https://rustup.rs) (stable) for host builds.
- [Docker](https://docs.docker.com/get-docker/) for cross-compilation via the `rust-zig-builder`
  container (used by the aarch64/RPi build).

## Building

Run from the repo root:

```bash
cargo build -p pi-beep                                          # host build
cargo build -p pi-beep --target=aarch64-unknown-linux-musl      # cross-compile for RPi
ARCH=aarch64 PROFILE=release ./shared-libs/crates/pi-beep/build-pi-beep.sh      # OS-image build path
```

## Testing

Run from the repo root:

```bash
cargo test -p pi-beep
```

There are no tests in the source today (the logic is direct sysfs I/O that only exercises on RPi
hardware). If you add testable logic, add unit tests alongside it.

## Formatting

Run from the repo root:

```bash
make start-core-format         # format the shared Rust crates (incl. pi-beep)
make start-core-format-check   # read-only check (what CI runs)
```

The monorepo formats with nightly rustfmt; `make start-core-format` covers it.
