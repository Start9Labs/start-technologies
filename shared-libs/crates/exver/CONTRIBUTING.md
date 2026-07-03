# Contributing to exver

How to build, test, and change the `exver` crate.

## Documentation

- README.md — what the crate is and how to use it (version format, range grammar, `satisfies`,
  algebraic laws).
- ARCHITECTURE.md — how it works internally (types, parsing, satisfiability, wasm surface).
- CONTRIBUTING.md — this file: toolchain and workflow.
- AGENTS.md — agent-facing rules and gotchas. `CLAUDE.md` is a one-line `@AGENTS.md` import.

## Prerequisites

- Rust stable (the workspace toolchain) for the Rust library and tests.
- For the WebAssembly build/publish: `wasm-pack` (and `jq` + `npm` for `publish.sh`).

## Building

From the repo root:

```bash
cargo build -p exver
cargo build -p exver --features wasm
```

## Testing

From the repo root:

```bash
cargo test -p exver
cargo test -p exver --features wasm
```

`src/test.rs` runs proptest property tests over the `VersionRange` laws (commutativity,
associativity, identity, annihilator, distributivity, De Morgan). Failing seeds are persisted
under `proptest-regressions/` — commit those if a new regression case is found.

## Formatting

From the repo root (this crate formats as part of the shared Rust crates):

```bash
make start-core-format
make start-core-format-check
```

## Publishing the wasm package

`./publish.sh` builds with `wasm-pack` and publishes to npm as `@start9labs/exver`. It copies
`exver.d.ts` into the generated `pkg/` (gitignored). Run it only when intentionally cutting an
npm release.

## Making a change

- If you change the version or range string format, update `src/grammar.pest` and the `FromStr`
  impls together, and update README.md/ARCHITECTURE.md to match.
