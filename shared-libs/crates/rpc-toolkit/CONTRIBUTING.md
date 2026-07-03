# Contributing to rpc-toolkit

How to build, test, and change `rpc-toolkit`, a first-party crate in the start-technologies monorepo.

## Documentation

- **README.md** — what the crate is and how to use it.
- **ARCHITECTURE.md** — how it works internally (handlers, server, CLI, values).
- **CONTRIBUTING.md** — this file: workflow, build/test/format, PR conventions.
- **AGENTS.md** — rules and file map for agents working in this crate. `CLAUDE.md` is a one-line
  `@AGENTS.md` import; never put content in `CLAUDE.md`.

## Prerequisites

- Rust stable. No nightly toolchain is required to build or test.
- The `ts-rs` optional feature pulls in `ts-rs 9` for TypeScript generation; only needed when
  working on the TS-output path.

## Building

From the repo root:

```bash
cargo build -p rpc-toolkit
cargo build -p rpc-toolkit --features ts-rs
cargo build -p rpc-toolkit --no-default-features   # JSON-only (no CBOR)
```

This is **not** a vendored fork — it is a first-party crate consumed via a path dependency
(currently by `start-core`). When you change a public API, update the consumers in the same change
and check `cargo build -p start-core` still passes.

## Testing

From the repo root:

```bash
cargo test -p rpc-toolkit
```

Integration tests live in `tests/test.rs`. When you touch dispatch, handler composition, or the
CLI/server bindings, exercise it there.

## Formatting

This crate ships a `rustfmt.toml` (`group_imports = "StdExternalCrate"`,
`imports_granularity = "Module"`). Format with the shared Rust make target from the repo root:

```bash
make start-core-format         # format all shared Rust crates (incl. rpc-toolkit)
make start-core-format-check   # CI read-only check
```

The import-grouping options in `rustfmt.toml` are applied either way.
