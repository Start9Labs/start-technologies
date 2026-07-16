# Contributing to StartTunnel

StartTunnel is one product in the `start-os` monorepo. Start with the root
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) for environment setup (Docker, Rust
toolchain, Node), collaboration channels, and repo-wide conventions. This file
covers what is specific to building, testing, and changing StartTunnel.

## Documentation

- [`README.md`](README.md) — what StartTunnel is and how to use it.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — how the code is laid out and how a
  request flows through the system.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — this file: building, testing, and
  changing StartTunnel.
- [`AGENTS.md`](AGENTS.md) — rules for AI agents working in this scope.
  `CLAUDE.md` is a one-line `@AGENTS.md` import.

## Prerequisites

- Read [`AGENTS.md`](AGENTS.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md).
- Most backend work happens in `shared-libs/crates/start-core/src/tunnel/`, **not** in
  this directory. The `projects/start-tunnel/` dir is a thin wrapper (entry point, UI,
  systemd unit, docs).

## Building (from the repo root)

```bash
make start-tunnel                                   # full daemon build (UI + tunnelbox)
cargo build -p start-tunnel --bin tunnelbox   # cargo only (UI must be prebuilt)
cargo check -p start-tunnel                    # fast type-check
npm run build:tunnel                          # build just the Angular UI (no make target; make start-tunnel chains it)
make start-tunnel-deb                                # Debian package
```

`make start-tunnel` builds the UI, compresses it into
`web/dist/static/start-tunnel/`, then compiles `tunnelbox`, which embeds that UI
via `include_dir!`. Output:
`target/<arch>-unknown-linux-musl/<profile>/tunnelbox`.

## Testing

```bash
make start-core-test    # backend tests — tunnel logic lives in start-core
```

Tunnel behavior is exercised by the `start-core` test suite (this crate is a
wrapper with no independent tests). For runtime verification, build the `.deb`
and install it on a Debian 13 VPS, or use a local VM. Backend changes should be
checked against the full CI matrix concerns — see the cross-platform note below.

## Formatting

```bash
make start-tunnel-format        # format the tunnel Rust crate
make start-tunnel-format-check  # verify (what CI runs)
```

The tunnel crate is Rust (edition 2024). The tunnel web app formats with the
rest of the Angular workspace via `make web-format`.

## Cross-platform

Local `cargo check`/`build` only covers Linux. CI also builds
`x86_64`/`aarch64-apple-darwin` and `riscv64`/`aarch64`/`x86_64` linux-musl. Any
change touching Rust dependencies or `libc`/platform APIs needs the darwin
target considered. For a code path that is dead on the other platform, `cfg`-gate
it rather than reimplementing cross-platform.

## Versioning

The crate version lives in `Cargo.toml` (marked with `# VERSION_BUMP`) and
mirrors the top `CHANGELOG.md` heading, which is the **prospective next version**
(`## [X.Y.Z]`), not `## [Unreleased]`. If that version has no `start-tunnel/v*`
tag on origin yet, it is unreleased — add entries under it, and raise the number
(and `Cargo.toml`) only for a larger semver tier; cut a new heading only once the
current one is tagged. Follows Keep a Changelog; see the root
[`AGENTS.md`](../../AGENTS.md) for the full rule.

## Making a change

- If you change the db schema, add a numbered migration in
  `shared-libs/crates/start-core/src/tunnel/migrations/` and register it in
  `shared-libs/crates/start-core/src/tunnel/migrations/mod.rs`.
- If you change the API, regenerate TS bindings: `make start-core-ts-bindings`.
- For user-facing behavior (UI, CLI flags/output, install flow,
  subnets/devices/forwarding), the docs live in `docs/src/` and publish to
  `start9.com/start-tunnel/`. Add a `CHANGELOG.md` entry under the changelog's
  current top (prospective-version) heading — see Versioning above.
