# AGENTS.md — imbl-value

`imbl-value` is a first-party library crate in the start-technologies monorepo (`shared-libs/crates/imbl-value`).
It provides a JSON `Value` type that mirrors `serde_json::Value` but is backed by the persistent
immutable data structures from the [`imbl`](https://crates.io/crates/imbl) crate, so clones are cheap
and structural sharing is preserved. `CLAUDE.md` is a one-line `@AGENTS.md` import — edit this file,
not that one. See [ARCHITECTURE.md](ARCHITECTURE.md) for how it works and [CONTRIBUTING.md](CONTRIBUTING.md)
for workflow.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/lib.rs` — the `Value` enum (`Null`, `Bool`, `Number`, `String(Arc<String>)`, `Array(Vector<Value>)`,
  `Object(InOMap<InternedString, Value>)`) and its core surface: `get`/`get_mut`, the `as_*` checkers/accessors,
  JSON-pointer ops, `Index`-trait indexing, `Display`/`Debug`, `Eq`/`Ord`/`Hash`, conversion to/from
  `serde_json::Value`, and the `to_value`/`from_value` helpers. Re-exports `imbl`, `InOMap`,
  `serde_json::{Number, Error as ErrorSource}`, and `yasi::InternedString`.
- `src/ser.rs` — `Serialize` impl for `Value` plus a custom `Serializer` that turns any `T: Serialize` into a `Value`.
- `src/de.rs` — `Deserialize` impl for `Value` with the `MapAccess`/`SeqAccess` visitors.
- `src/from.rs` — `From`/`Into` impls: `serde_json::Value` ↔ `Value`, and slice/array/`str` → `Value`.
- `src/index.rs` — the `Index` trait and impls for `usize`/`&str` indexing, plus the `parse_index` helper.
- `src/macros.rs` — the `json!` macro (and `json_internal!`/`json_internal_vec!`/`json_unexpected!` helpers)
  for ergonomic `Value` construction.
- `src/in_order_map/mod.rs` — `InOMap<K, V>`, an insertion-order-preserving persistent map backed by
  `imbl::Vector<(K, V)>`, with get/insert/remove/update plus union/difference/intersection set ops.
- `src/in_order_map/my_visitor.rs` — `MyVisitor`, the serde `Visitor` for deserializing an `InOMap`.
- `src/arbitrary.rs` — proptest `Arbitrary` impl and `value_strategy`/`number_strategy`/`array_strategy`/`object_strategy`.
  Gated behind the `arbitrary` feature.
- `src/ts_rs.rs` — `ts-rs` `TS` impls for `Value` and `InOMap` for TypeScript type generation. Gated behind the `ts-rs` feature.

## Build & test (run from the repo root)

```bash
cargo build -p imbl-value                       # build the library
cargo test  -p imbl-value                       # run the test suite
cargo build -p imbl-value --features arbitrary  # build with the proptest strategies
cargo build -p imbl-value --features ts-rs      # build with ts-rs TS impls
```

## Gotchas

- **`InOMap` is order-preserving, not a hash map.** It is backed by `imbl::Vector<(K, V)>`, so lookups are
  O(n), not O(1). It exists to keep object keys in insertion order through deserialize → serialize round-trips.
- **`Value::String` is `Arc<String>`** — clones are cheap (refcount bump). The same cheap-clone property
  holds for `Array`/`Object` via `imbl`'s structural sharing.
- **Object keys are `yasi::InternedString`, not `String`.** `Object` is `InOMap<InternedString, Value>`;
  use the re-exported `imbl_value::InternedString` for keys.
- **(De)serialization is derived from `serde_json`.** `ser.rs`/`de.rs` (and `index.rs`, `macros.rs`) are
  adapted from `serde_json`'s `value` module, rewritten against the `imbl`-backed `Value`. Keep `ser.rs` and
  `de.rs` in sync with each other, and prefer porting upstream fixes over inventing new behavior. The
  attribution lives in [LICENSE](LICENSE) — keep it if you touch these files.
- **`json!` uses `local_inner_macros`** to avoid namespace pollution; the `json_internal_vec!` helper is
  defined outside that scope so `vec!` resolves correctly.
- **Optional features:** `ts-rs` is required by the `start_core` consumer; `arbitrary` is used by `json-patch`
  for proptest-based testing. Both are off by default.
- **`Display` doubles as compact and pretty.** It adapts a `fmt::Formatter` as an `io::Write` sink; `{}` is
  compact and `{:#}` is pretty.
