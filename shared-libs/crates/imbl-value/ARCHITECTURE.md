# imbl-value Architecture

`imbl-value` is a JSON `Value` type built on persistent immutable data structures. It mirrors the shape
of `serde_json::Value` but swaps in `imbl`'s structural-sharing collections, so cloning a large value is
cheap and snapshots of a tree share memory with their predecessors. It is the JSON-value backbone that
several other first-party crates build on.

## Place in the monorepo

- **Path:** `shared-libs/crates/imbl-value`
- **Package name:** `imbl-value` (build/test with `cargo … -p imbl-value`)
- **Crate type:** library (`src/lib.rs`)
- **Edition:** 2021
- **Consumers (first-party):** `patch-db`, `json-patch`, `json-ptr`, `jsonpath_lib`, `rpc-toolkit`, and
  `start_core`. Each depends on it as a direct path dependency — there is no `[patch]` redirection.
- **Key external deps:** `imbl` (with `serde`, `small-chunks`), `serde`/`serde_json`, and `yasi` (path dep,
  for `InternedString` object keys).

## The Value type

```rust
pub enum GenericValue<K: ObjectKind> {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(Arc<String>),
    Array(imbl::Vector<GenericValue<K>>),
    Object(K::Map),
}

pub type Value = GenericValue<InOrder>;   // objects are InOMap<InternedString, Value>
pub type OrdValue = GenericValue<Sorted>; // objects are imbl::OrdMap<InternedString, OrdValue>
```

The variants are a one-to-one analogue of `serde_json::Value`, with three deliberate substitutions:

- arrays use `imbl::Vector` instead of `Vec`,
- objects use the map `K` selects instead of `Map`: `InOMap` (this crate) keeps insertion order, `imbl::OrdMap`
  keeps keys sorted with O(log n) lookups,
- strings are `Arc<String>` for cheap shared ownership.

`ObjectKind` (`object.rs`) names the map type, and `ObjectMap` is the set of map operations the crate's
generic code uses. Every impl is written once over `GenericValue<K>`; `Value` and `OrdValue` convert into each
other with `From`.

`Number` is re-used directly from `serde_json`, so numeric semantics match `serde_json` exactly. `lib.rs`
also re-exports `imbl`, `InOMap`, `serde_json::{Number, Error as ErrorSource}`, and `yasi::InternedString`
so consumers don't need to add those crates themselves.

## Module map and data flow

- **`lib.rs`** holds the enum and its whole core surface: typed accessors (`as_*`, `get`, `get_mut`),
  JSON-pointer navigation, `Index`-trait indexing, `Display`/`Debug`, the `Eq`/`Ord`/`Hash` impls, and the
  `to_value`/`from_value` round-trip helpers. `to_value::<T>(&T) -> Result<Value, Error>` (or `to_ord_value`
  for an `OrdValue`) and `from_value::<T>(value) -> Result<T, Error>` are the primary entry points for
  converting between a value and arbitrary `Serialize`/`Deserialize` types.
- **`ser.rs` / `de.rs`** implement (de)serialization for `Value` by hand rather than deferring to
  `serde_json`. `ser.rs` also defines a custom `Serializer<K>` that drives the `T -> GenericValue<K>`
  conversion behind `to_value`; `de.rs` defines the visitors that build a `Value` from any deserializer.
- **`from.rs`** carries the cheap, infallible conversions: `serde_json::Value` ↔ `GenericValue`,
  `Value` ↔ `OrdValue`, and slices/arrays/`&str` → `GenericValue`.
- **`index.rs`** implements indexing (`value["key"]`, `value[0]`) via the `Index` trait. JSON-pointer
  navigation in `lib.rs` uses `parse_index` to read array indices.
- **`macros.rs`** provides the `json!` macro for building a `Value` from JSON-like literal syntax, expanded
  through `json_internal!` and friends.

## InOMap

`InOMap<K, V>` (`in_order_map/mod.rs`) is the object backing store. It is a persistent, insertion-order
preserving map implemented over `imbl::Vector<(K, V)>`. Order preservation is the whole point: a JSON object
deserialized into a `Value` and serialized back out keeps its keys in the original order. The trade-off is
that lookups scan the vector (O(n)). It exposes the usual get/insert/remove/update methods plus
union/difference/intersection set operations, and `my_visitor.rs` supplies the serde `Visitor` that
constructs one during deserialization.

## Optional features

- **`arbitrary`** pulls in `proptest`/`proptest-derive` and enables `imbl/proptest`, adding the `Arbitrary`
  impl and the strategy functions in `arbitrary.rs`. Used by `json-patch` for property-based tests.

`arbitrary` is off by default. RPC owns `Value`'s `unknown` and `InOMap`'s map
bridges in its `ts` consumer, using a one-way dependency on this crate.

## Further reading

- [README.md](README.md) — what the crate is and how to use it.
- [AGENTS.md](AGENTS.md) — layout, build/test commands, and gotchas for agents.
