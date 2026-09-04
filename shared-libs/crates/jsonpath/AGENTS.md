# AGENTS.md — jsonpath

A JsonPath query engine for selecting, deleting, and transforming JSON values. It originated as a
fork of [freestrings/jsonpath](https://github.com/freestrings/jsonpath) but has since **fully
diverged** — it is maintained as first-party code in the start-technologies monorepo at
`shared-libs/crates/jsonpath`, with no intent to sync back upstream. The Cargo package name is
`jsonpath_lib` (it differs from the directory name `jsonpath`), so all cargo commands use
`-p jsonpath_lib`. `CLAUDE.md` is a one-line `@AGENTS.md` import — don't edit it.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how it's built and [CONTRIBUTING.md](CONTRIBUTING.md) for
how to contribute. [README.md](README.md) is the usage/API reference.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/lib.rs` — public crate API: the convenience functions (`select`, `selector`, `selector_as`,
  `select_as`, `select_as_str`, `delete`, `replace_with`) and the `PathCompiled` pre-compiled
  expression type. Also re-exports `JsonSelector` / `JsonSelectorMut` and the deprecated
  `compile()` / `Compiled` / `Parser` / `Selector` / `SelectorMut` / `JsonPathError`. The
  crate-level doc test lives here.
- `src/paths/` — **preferred** path parsing and tokenization. `path_parser.rs` (`PathParser`),
  `tokenizer.rs`, `tokens.rs`, `str_reader.rs` (`StrRange`), `parser_token_handler.rs`,
  `parser_node_visitor.rs`. `mod.rs` re-exports `PathParser`, `StrRange`, `TokenError`, etc.
- `src/selector/` — **preferred** high-level query API. `selector_impl.rs` defines `JsonSelector`
  and `JsonSelectorMut` (select / filter / transform / delete); `terms.rs`, `value_walker.rs`,
  `cmp.rs`, `utils.rs` support it.
- `src/select/` — **deprecated** low-level selection engine (filter expressions, comparison
  operators, value walking) backing the old `Selector` / `SelectorMut`. `expr_term.rs`, `cmp.rs`,
  `value_walker.rs`, `mod.rs`.
- `src/parser/` — **deprecated** legacy AST parser (`tokenizer.rs`, `path_reader.rs`, `mod.rs`),
  used only by the deprecated `compile()` and `Compiled`.
- `src/ffi/mod.rs` — **deprecated** C FFI bindings (`ffi_select`, `ffi_path_compile`, …). Do not add
  new FFI here.
- `wasm/`, `benchmark/` — auxiliary build targets (separate Cargo manifests, own READMEs). `lua/`,
  `docs/`, `*.sh` build scripts are inherited from the original project and unused by the Rust crate.
- `tests/` — one integration-test crate (`array_filter`, `common`, `filter`,
  `jsonpath_examples`, `lib`, `op`, `paths`, `precompile`, `readme`, `return_type`, `selector`).

## Build & test (run from the repo root)

```bash
cargo build -p jsonpath_lib          # build the library (rlib + cdylib)
cargo build -p jsonpath_lib --lib    # build only the rlib, skip the cdylib
cargo test  -p jsonpath_lib          # unit + integration + doc tests
cargo test  -p jsonpath_lib --lib    # lib unit tests only
cargo test  -p jsonpath_lib --test '*'   # integration tests only
cargo test  -p jsonpath_lib selector_delete   # a single test by name
```

## Gotchas

- **Diverged fork (history, not an ongoing concern).** It began as `freestrings/jsonpath` but has
  fully diverged; there is no upstream to track or reconcile with — treat it as first-party and edit
  freely. The largest divergence is operating on `imbl_value::Value` instead of `serde_json::Value`.
- **Edition 2015.** `Cargo.toml` sets no `edition`, so it defaults to 2015 — old `extern crate`
  syntax, two-element `use {a, b}` paths, etc.
- **Package name ≠ dir name.** The crate is `jsonpath_lib`; cargo `-p` flags must use that.
- **`cdylib` + `rlib`.** The lib emits both crate types; the `cdylib` feeds the `wasm/` bindings.
  Changing public types may require rebuilding those bindings.
- **Two parser stacks.** Prefer the `paths`/`selector` APIs (`select`, `selector`, `PathCompiled`)
  over the deprecated `parser`/`select` ones (`compile`, `Compiled`). Note `start-core` still
  consumes the deprecated `Compiled` type.
- **Built on `imbl-value`, not `serde_json::Value`.** Query results are `imbl_value::Value` /
  `Vector`, so doc examples use `#[macro_use] extern crate imbl_value;` and `vector![]`.
