# Contributing to imbl-value

`imbl-value` is a first-party library crate at `shared-libs/crates/imbl-value` in the start-technologies monorepo.

## Documentation

- **README.md** — what the crate is and how to use it.
- **ARCHITECTURE.md** — how it works: the `Value` enum, the modules, `InOMap`, and the optional features.
- **CONTRIBUTING.md** — this file: toolchain and workflow.
- **AGENTS.md** — rules and layout notes for AI agents. `CLAUDE.md` is a one-line `@AGENTS.md` import; don't
  put content in it — edit `AGENTS.md` instead.

## Prerequisites

- Rust stable for building and testing.
- Nightly `rustfmt` for formatting (the `make start-core-format` target formats with nightly `rustfmt`).
- No extra tooling is required; the `arbitrary` (proptest) and `ts-rs` features pull their deps in
  automatically when enabled.

## Building

From the repo root:

```bash
cargo build -p imbl-value
cargo build -p imbl-value --features arbitrary
cargo build -p imbl-value --features ts-rs
```

## Testing

From the repo root:

```bash
cargo test -p imbl-value
```

If you touch the proptest strategies, also run with `--features arbitrary`.

Remember the downstream blast radius: `Value` is consumed by `patch-db`, `json-patch`, `json-ptr`,
`jsonpath_lib`, `rpc-toolkit`, and `start_core`. A change to the public surface or to (de)serialization
behavior can ripple into all of them — build the affected consumers if in doubt.

## Formatting

From the repo root:

```bash
make start-core-format         # format the shared Rust crates (incl. imbl-value)
make start-core-format-check   # CI read-only check
```

Nightly is required — the workspace's rustfmt config relies on nightly-only options, which `make start-core-format` uses.
