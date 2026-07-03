# Contributing to jsonpath

How to build, test, and contribute to `jsonpath_lib` (the crate at `shared-libs/crates/jsonpath`).
For general monorepo setup, see the repo-root [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Documentation

This sub-tree's docs split across four files:

- `README.md` — what this is and how to use it (usage/API reference).
- `ARCHITECTURE.md` — how it's built (parse → walk flow, public API surface, place in the monorepo).
- `CONTRIBUTING.md` — this file; how to contribute.
- `AGENTS.md` — AI/dev operating rules and layout map (`CLAUDE.md` is a one-line `@AGENTS.md`
  import; don't edit it).

## Prerequisites

- [Rust](https://rustup.rs) (stable is sufficient to build and test). This is an **edition 2015**
  crate.
- Nightly toolchain only if you run the workspace formatter (`make start-core-format`).
- `imbl-value` is a sibling path dependency and builds automatically as part of the workspace.

## Building

Run from the repo root (the package name `jsonpath_lib` differs from the directory name):

```bash
cargo build -p jsonpath_lib          # rlib + cdylib
cargo build -p jsonpath_lib --lib    # rlib only (skip the cdylib)
```

## Testing

Run from the repo root:

```bash
cargo test -p jsonpath_lib                 # unit + integration + doc tests
cargo test -p jsonpath_lib --lib           # lib unit tests only
cargo test -p jsonpath_lib --test '*'      # integration tests only (tests/)
cargo test -p jsonpath_lib selector_delete # a single test by name
```

The `tests/` directory is a single integration-test crate with 11 modules; doc tests run from the
examples in `src/lib.rs` and `README.md`.

## Formatting

```bash
make start-core-format         # format the shared Rust crates (incl. jsonpath)
make start-core-format-check   # CI-style read-only check
```

The workspace uses nightly rustfmt. Note this crate carries pre-existing warnings (unused imports,
lifetime elisions) that predate the divergence; don't churn unrelated lines to silence them.

## Working in this crate

It began as a fork of [freestrings/jsonpath](https://github.com/freestrings/jsonpath) but has fully
diverged — there is no upstream to reconcile with, so edit it freely as first-party code. New code
should target the preferred `paths`/`selector` APIs, not the deprecated `parser`/`select`/`ffi`
modules. The main divergence from the original is operating on `imbl_value::Value` instead of
`serde_json::Value`.
