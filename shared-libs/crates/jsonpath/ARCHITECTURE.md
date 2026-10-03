# jsonpath Architecture

A JsonPath engine: it compiles a JsonPath string into tokens, then walks an `imbl_value::Value`
tree to select, filter, transform, or delete matching nodes. This document covers how the crate is
laid out and how a query flows through it. For usage and the full API surface, see
[README.md](README.md).

## Place in the monorepo

- **Path:** `shared-libs/crates/jsonpath`
- **Package name:** `jsonpath_lib` (the Cargo package name differs from the directory name
  `jsonpath`; always use `cargo … -p jsonpath_lib`). Library name is also `jsonpath_lib`.
- **Crate type:** `["cdylib", "rlib"]` — an rlib for Rust consumers plus a cdylib that backs the
  `wasm/` JavaScript/WebAssembly bindings.
- **Consumers:** `start-core` (`shared-libs/crates/start-core`) uses `jsonpath_lib::Compiled` in
  `src/config/hook.rs` for configuration-hook processing.
- **First-party.** A direct path dependency (`jsonpath_lib = { path = "../jsonpath" }` in
  start-core's `Cargo.toml`) — no `[patch]`, no crates.io pull. It originated as a fork of
  [freestrings/jsonpath](https://github.com/freestrings/jsonpath) and has since fully diverged; it is
  maintained as first-party code with no upstream sync.
- **Dependencies:** `imbl-value` (sibling crate; supplies the immutable `Value`/`Vector` types this
  engine operates on), `serde` / `serde_json`, and `log`.

## How a query flows

A JsonPath string is processed in two stages — **parse** then **walk**:

1. **Parse.** `paths::PathParser::compile(path)` tokenizes the path (`paths::tokenizer`,
   `paths::str_reader::StrRange`) and builds the token tree consumed during selection
   (`paths::tokens`, `parser_node_visitor`, `parser_token_handler`). Compilation is fallible and
   returns `TokenError` on a malformed path.
2. **Walk.** `selector::JsonSelector` (read-only) or `selector::JsonSelectorMut` (mutating) takes a
   compiled `PathParser` and an `imbl_value::Value`, then evaluates the tokens against the value
   tree: recursive descent, wildcards, array indexing/ranges/unions, and filter expressions
   (`selector::terms`, `selector::value_walker`, `selector::cmp`). Read selectors yield
   `Vector<&Value>`; mutating selectors return a rebuilt `Value` for `delete` / `replace_with`.

## Public API surface (`src/lib.rs`)

The crate root wraps the parse-then-walk pair behind convenience functions and one pre-compiled
type:

- **One-shot functions** (compile the path on each call): `select`, `select_as`, `select_as_str`,
  `delete`, `replace_with`.
- **Reusable closures** (compile once, query many): `selector`, `selector_as` — return an
  `FnMut(&str | ())` over a bound JSON value.
- **`PathCompiled<'a>`** — the preferred pre-compiled expression. `PathCompiled::compile(path)`
  parses once; its `select` reuses the compiled form across many JSON inputs.
- **Selectors:** `JsonSelector` / `JsonSelectorMut`, re-exported from `selector`, for callers that
  want direct control over the walk stage.
- **`JsonPathError`** — the error type (`Path`, `Serde`, … variants); marked deprecated upstream but
  still the returned error.

### Deprecated surface

The crate also re-exports an older stack, kept for compatibility: `compile()` and the `Compiled`
struct (backed by `parser` + `select`), plus `Parser`, `Selector`, `SelectorMut`. `start-core`
currently depends on `Compiled`. New code should prefer `PathCompiled` and the `paths`/`selector`
modules; the `parser`, `select`, and `ffi` modules are deprecated (the `ffi` C bindings moved to
`wasm/` upstream).

## Supported path syntax

Root (`$`), recursive descent (`..`), wildcards (`*`), array indexing (`[n]`, negative indices),
ranges (`[n:m]`), unions (`[n,m,k]`), filters (`[?(@.field < 10)]`), and field access
(`$.store.book[*].author`). See the doc test at the top of `src/lib.rs` and `tests/` for worked
examples.

## Further reading

- [README.md](README.md) — usage and API examples (Rust, plus the JS/WASM bindings inherited from the original project).
- [AGENTS.md](AGENTS.md) — toolchain, build/test/format, conventions.
- Sibling: [`imbl-value`](../imbl-value) — the value types this engine queries.
