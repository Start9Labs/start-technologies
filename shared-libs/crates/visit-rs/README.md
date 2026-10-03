# visit-rs

Value and type reflection shared by Start9's Rust products. The runtime crate
re-exports its proc macros from the sibling `visit-rs-derive` crate. Both are
members of the root Cargo workspace; start-core and StartWRT use path dependencies.

## Traversal

Implement `Visitor` and `Visit` for the values a visitor accepts. `VisitFields`
derives struct traversal; `VisitVariants` derives enum traversal. The generated
traits cover direct/covered/static values, named fields and synchronous/asynchronous
visitors. Iterators and streams are lazy: consume them to perform the visits.

`StructInfo` and `EnumInfo` describe names and field/variant counts. `Named`
provides a field name; `Static<T>` provides a type marker without constructing a
value. `#[visit(skip)]` removes a field and its visiting bounds; tuple indices still
refer to the original Rust fields. Reflection names use serialization-oriented
serde renames or `visit` renames, and serialization skips remove reflected fields.

Examples under `examples/` demonstrate the different traversal forms. Enum variant
wrappers use higher-ranked lifetime bounds; static/variant visiting requires owned
`'static` types. RPC JSON shapes use the separate `SerdeShape` derive below.

## Serde JSON shapes

`#[derive(SerdeShape)]` implements `shape::SerdeShape<V>` for a `ShapeVisitor`.
`Direction::Input` selects deserialization rules; `Direction::Output` selects
serialization rules. The visitor receives struct styles, variant tags, field names,
aliases, defaults and flattening. Unit, empty tuple and empty named shapes remain
distinct. Conversions traverse their wire types. Custom hooks require an explicit
`#[visit(wire = "RustType")]` representation. `input_wire` and `output_wire`
select representations for just one direction, on fields or containers. Reuse the
actual deserialization representation for custom normalizers.

Serde's own derive metadata owns parsing, validation and rename rules. Shape
traversal does not change the existing value-reflection names or erase fields used
by only one JSON direction. `rpc-toolkit` uses it for optional TypeScript bindings;
see [its guide](../rpc-toolkit/docs/typescript.md).

## TypeScript

The `ts` feature exposes `TS`, `TSVisitor` and `#[derive(TS)]`. Both derives use
one serde lowering implementation. `TSVisitor::with_direction` selects the wire
contract; named input declarations append `Input` to the output alias. Override a
colliding input alias with `#[ts(input_rename = "RequestName")]`.

`#[ts(export)]` registers an annotation-owned root. `export_namespace(crate_name,
namespace)` collects its input/output declarations and transitive dependencies
into one checked module. `#[ts(namespace = ["", "tunnel"])]` exports one owner
into both namespaces. `#[ts(namespace = "tunnel")]` selects a separate export
collection. Duplicate aliases and recursive inline types are errors.

Container attributes also include `rename`, `concrete(T = RustType)`, `wire`
and `type`. Field `wire`/`type` overrides reuse the shape lowering; `skip` excludes
an injected/internal named field from the public projection. Generic types inline
unless a concrete export supplies a declaration name. A literal override owns
its JSON contract and must account for nullable options explicitly.

Bindings describe structural JSON, not every application validator or CBOR value.
Optional input fields and nullable output fields are distinct: SDK producers use
input declarations, not serialized response DTOs.

## Features

- `serde` (default): serde serializers act as value visitors.
- `ts`: shared TypeScript rendering, derive and annotation-owned exports.
- `chrono`, `ipnet`, `josekit`, `url`, `yajrc`: optional external JSON representations.
- `meta` (default): exposes raw structured attribute metadata and forwards the
  corresponding derive feature. Directional JSON-shape traversal works with either
  feature disabled.

## Development

Run `make rpc-toolkit-test` from the repository root for reflection and binding
feature matrices, or `cargo test -p visit-rs` for this crate. See [AGENTS.md](AGENTS.md)
for scoped contributor rules and [ARCHITECTURE.md](ARCHITECTURE.md) for ownership.

## Provenance and license

Migrated from [dr-bonez/visit-rs](https://github.com/dr-bonez/visit-rs), including the
metadata/generic-enum work from [PR #1](https://github.com/dr-bonez/visit-rs/pull/1).
Maintained here under MIT; see [LICENSE](LICENSE).
