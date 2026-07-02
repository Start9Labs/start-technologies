# start-core

The shared Rust backend library for StartOS. Cargo package **`start-core`**, library name **`start_core`**.

Every Start9 product binary links against this crate. It holds the entire backend: service
management, the s9pk package format, networking, the patch-db data model, the registry, the
tunnel server, install/update, backup, signing, and more. The product binaries themselves are
thin wrappers that select which entrypoints to enable from `start_core::bins`.

## Place in the monorepo

This crate lives at `shared-libs/crates/start-core` and is the lib that all six product bins depend on:

| Binary | Defined in | Notes |
|--------|-----------|-------|
| `startbox` / `startd` | `projects/start-os/src/bin/startbox.rs` | Main OS daemon |
| `start-container` | `projects/start-os/src/bin/start-container.rs` | Runs inside package LXC containers |
| `start-cli` | `projects/start-cli/src/main.rs` | CLI over the daemon's JSON-RPC API |
| `registrybox` | `projects/start-registry/src/main.rs` | Package registry server |
| `tunnelbox` | `projects/start-tunnel/src/main.rs` | StartTunnel VPN/forwarding server |
| `startwrt` | `projects/start-wrt/backend/ctrl/src/bin/startwrt.rs` | StartWRT router OS daemon+CLI |

Each wrapper builds a `start_core::bins::MultiExecutable`, enables the entrypoints it wants
(`enable_startd`, `enable_start_cli`, `enable_start_container`, `enable_start_registry`,
`enable_start_tunnel`, …), and calls `.execute()`. The exception is `startwrt`: a full backend
of its own that imports this crate aliased as `startos` for its `net`/`util` primitives rather
than enabling `MultiExecutable` entrypoints.

## Requirements

- [Rust](https://rustup.rs) (nightly toolchain for formatting)
- [rust-analyzer](https://rust-analyzer.github.io/) recommended
- [Docker](https://docs.docker.com/get-docker/) (for the `rust-zig-builder` cross-compile container used by the test/build scripts)

## Quickstart

Run from the repo root (the single Cargo workspace):

```bash
cargo check -p start-core          # type-check the library
make test-core                     # run the test suite (wraps run-tests.sh)
make format-core                   # nightly rustfmt on this crate
```

To build a product binary, build its crate, e.g. `cargo build -p start-os --bin startbox`.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — module map, RPC pattern, patch-db data flow
- [CONTRIBUTING.md](CONTRIBUTING.md) — build, test, format, and contribution workflow
- [AGENTS.md](AGENTS.md) — operating rules for AI/dev work in this crate

Topical deep-dives: [rpc-toolkit.md](rpc-toolkit.md), [patchdb.md](patchdb.md),
[i18n-patterns.md](i18n-patterns.md), [core-rust-patterns.md](core-rust-patterns.md),
[s9pk-structure.md](s9pk-structure.md), [exver.md](exver.md), [VERSION_BUMP.md](VERSION_BUMP.md).
