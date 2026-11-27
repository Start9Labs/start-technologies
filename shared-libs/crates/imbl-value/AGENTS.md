# AGENTS.md — imbl-value

`imbl-value` is a first-party library crate in the start-technologies monorepo (`shared-libs/crates/imbl-value`).
It provides a JSON `Value` type that mirrors `serde_json::Value` but is backed by the persistent
immutable data structures from the [`imbl`](https://crates.io/crates/imbl) crate, so clones are cheap
and structural sharing is preserved. `CLAUDE.md` is a one-line `@AGENTS.md` import — edit this file,
not that one. See [ARCHITECTURE.md](ARCHITECTURE.md) for how it works.

## Layout

- `src/lib.rs` — the `GenericValue<K>` enum (`Null`, `Bool`, `Number`, `String(Arc<String>)`,
  `Array(Vector<GenericValue<K>>)`, `Object(K::Map)`), its aliases `Value` (`InOMap` objects) and `OrdValue`
  (`imbl::OrdMap` objects), and its core surface: `get`/`get_mut`, the `as_*` checkers/accessors,
  JSON-pointer ops, `Index`-trait indexing, `Display`/`Debug`, `Eq`/`Ord`/`Hash`, and the
  `to_value`/`to_ord_value`/`from_value` helpers. Re-exports `imbl`, `InOMap`,
  `serde_json::{Number, Error as ErrorSource}`, and `yasi::InternedString`.
- `src/object.rs` — `ObjectKind` (selects a value's object map: `InOrder` or `Sorted`) and `ObjectMap`, the map
  operations the rest of the crate uses on objects.
- `src/ser.rs` — `Serialize` impl for `GenericValue` plus a custom `Serializer<K>` that turns any `T: Serialize`
  into a `GenericValue<K>`.
- `src/de.rs` — `Deserialize` impl for `GenericValue`, and one by-reference `Deserializer` that both
  `GenericValue` and `&GenericValue` forward to: the owned form copies strings, the borrowed form lends them.
- `src/from.rs` — `From`/`Into` impls: `serde_json::Value` ↔ `GenericValue`, `Value` ↔ `OrdValue`, and
  slice/array/`str` → `GenericValue`.
- `src/index.rs` — the `Index` trait and impls for `usize`/`str`/`String` indexing.
- `src/macros.rs` — the `json!` macro (and `json_internal!`/`json_internal_vec!`/`json_unexpected!` helpers)
  for ergonomic `Value` construction, and `ord_json!`, which builds an `OrdValue` from the same syntax.
- `src/in_order_map/mod.rs` — `InOMap<K, V>`, an insertion-order-preserving persistent map backed by
  `imbl::Vector<(K, V)>`, with get/insert/remove/update plus union/difference/intersection set ops.
- `src/in_order_map/my_visitor.rs` — `MyVisitor`, the serde `Visitor` for deserializing an `InOMap`.
- `src/arbitrary.rs` — proptest `Arbitrary` impl and `value_strategy`/`number_strategy`/`array_strategy`/`object_strategy`.
  Gated behind the `arbitrary` feature.
- RPC's `ts` consumer owns TypeScript bridges for `Value` and `OrdValue` (`unknown`) and `InOMap` (map).

## Build & test (run from the repo root)

```bash
cargo build -p imbl-value                       # build the library
cargo test  -p imbl-value                       # run the test suite
cargo build -p imbl-value --features arbitrary  # build with the proptest strategies
```

## Gotchas

- **`InOMap` is order-preserving, not a hash map.** It is backed by `imbl::Vector<(K, V)>`, so lookups are
  O(n), not O(1). It exists to keep object keys in insertion order through deserialize → serialize round-trips.
- **`Value::String` is `Arc<String>`** — clones are cheap (refcount bump). The same cheap-clone property
  holds for `Array`/`Object` via `imbl`'s structural sharing.
- **Object keys are `yasi::InternedString`, not `String`.** `Value`'s objects are `InOMap<InternedString, Value>`
  and `OrdValue`'s are `OrdMap<InternedString, OrdValue>`; use the re-exported `imbl_value::InternedString`
  for keys.
- **Deserializing an owned value never takes it apart.** `from_value` walks the value by reference, so a
  value shared with another (a database snapshot, a clone) costs the same as `T::deserialize(&value)`.
  Both forms fall back to `deserialize_any` when asked for a number they don't hold.
- **Write value behavior once, against `GenericValue<K>`.** `Value` and `OrdValue` share every impl; reach
  objects through `ObjectMap` rather than a concrete map. `json!` builds a `Value`; convert with
  `OrdValue::from`.
- **(De)serialization is derived from `serde_json`.** `ser.rs`/`de.rs` (and `index.rs`, `macros.rs`) are
  adapted from `serde_json`'s `value` module, rewritten against the `imbl`-backed `Value`. Keep `ser.rs` and
  `de.rs` in sync with each other, and prefer porting upstream fixes over inventing new behavior. The
  attribution lives in [LICENSE](LICENSE) — keep it if you touch these files.
- **`json!` uses `local_inner_macros`** to avoid namespace pollution; the `json_internal_vec!` helper is
  defined outside that scope so `vec!` resolves correctly.
- **Optional feature:** `arbitrary` is used by `json-patch` for proptest-based testing
  and is off by default. TypeScript bridges belong to RPC; keep dependencies
  directed from that consumer to this crate.
- **`Display` doubles as compact and pretty.** It adapts a `fmt::Formatter` as an `io::Write` sink; `{}` is
  compact and `{:#}` is pretty.

## Consumer verification

Run `cargo test -p imbl-value --features arbitrary` when changing proptest strategies.
Public API or serialization changes also need checks of affected consumers:
`patch-db`, `json-patch`, `json-ptr`, `jsonpath_lib`, `rpc-toolkit`, and `start-core`.
