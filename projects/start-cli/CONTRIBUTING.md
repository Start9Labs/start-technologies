# Contributing to start-cli

`start-cli` is a thin `bin` crate over `start-core`. See the root
[CONTRIBUTING.md](../../CONTRIBUTING.md) for repo-wide workflow, commit, and review conventions;
this file covers what's specific to this crate.

## Documentation

- [`README.md`](./README.md) — what `start-cli` is and how to use it.
- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — how it's built (entrypoint, request flow, config).
- `CONTRIBUTING.md` — this file (how to contribute).
- [`AGENTS.md`](./AGENTS.md) — agent/dev rules; `CLAUDE.md` is a one-line `@AGENTS.md` import.

## Prerequisites

A Rust toolchain matching the workspace `edition = "2024"`, plus the local container backend
(Docker or Podman) if you exercise `s9pk` packaging. Everything builds from the monorepo root
(single Cargo workspace, single `Cargo.lock`).

## Building

From the monorepo root (single Cargo workspace, single `Cargo.lock`):

```sh
make start-cli                                             # build the start-cli bin
cargo build -p start-cli --bin start-cli             # dev shortcut (debug)
cargo build -p start-cli --bin start-cli --release   # dev shortcut (release)
cargo check -p start-cli                             # fast type-check
```

## Testing

No unit tests live in this crate. Exercise CLI behavior through `start-core` and by running
the built binary:

```sh
cargo test -p start-core                              # core tests (where CLI logic is tested)
cargo test -p start-core export_manpage_start_cli     # regenerate man pages
target/debug/start-cli --help                          # smoke test the bin
```

For end-to-end verification, build the binary and run it against a StartOS test VM (remote
commands need `-H https://<host>` + `auth login`; local commands like `s9pk`/`init-key`/`pubkey`
need no server).

## Formatting

Format this project from the monorepo root (CI runs the `start-cli-format-check` read-only variant):

```sh
make start-cli-format         # format start-cli
make start-cli-format-check   # check only (what CI runs)
cargo clippy -p start-cli
```

## Where code lives

This crate holds only `src/main.rs` and `Cargo.toml`. **Almost all changes belong in
`start-core`** (`shared-libs/crates/start-core`):

- A new or changed CLI command → `main_api()` in
  `shared-libs/crates/start-core/src/lib.rs` and the relevant `src/<area>/` module.
- CLI argument/config changes → `start-core::context::config` (`ClientConfig`).
- The CLI entrypoint behavior → `shared-libs/crates/start-core/src/bins/start_cli.rs`.

Edit this crate only when the entrypoint, Cargo features, or bin wiring change.

If your change touches the CLI surface a user or package author sees, update the relevant docs in
`projects/start-sdk/docs` (packaging) and the `projects/start-os/docs` in the same change set.
