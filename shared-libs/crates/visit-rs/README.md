# visit-rs

Generic value and type reflection shared by Start9's Rust products. The runtime
re-exports proc macros from `visit-rs-derive`; both are root Cargo workspace members.

## Value traversal

Implement `Visitor` and `Visit` for the values a visitor accepts. `VisitFields`
derives struct traversal; `VisitVariants` derives enum traversal. Direct, covered,
static, named and synchronous/asynchronous callbacks retain their existing contracts.
Iterators and streams are lazy: consume them to perform visits.

`StructInfo` and `EnumInfo` describe names and member counts. `Named` supplies a
field name; `Static<T>` supplies a type marker without constructing a value.
`visit(skip)` and `visit(rename)` control legacy value presentation; tuple indices
refer to original Rust fields. Serde skips and renames do not control reflection.
See `examples/` for value traversal forms and their lifetime requirements.

## Raw type reflection

`#[derive(visit_rs::VisitFields)]` for structs and
`#[derive(visit_rs::VisitVariants)]` for enums implement unconditional `TypeInfo`
declaration facts alongside their value and static traversal traits. `TypeInfo`
is available at the crate root and in `visit_rs::reflection`;
`VisitTypeAttributes<V>` is defined at the crate root.

```rust
#[derive(visit_rs::VisitFields)]
struct Record<'a, T, const N: usize> {
    name: &'a str,
    values: [T; N],
}

let declaration = <Record<'_, u32, 4> as visit_rs::TypeInfo>::DECLARATION;
assert_eq!(declaration.fields[0].name, Some("name"));
```

`TypeInfo::DECLARATION` provides `DeclarationInfo`: original Rust name, module,
declaration kind, lifetime/type/const parameters, source tokens, attributes, fields
and variants. `FieldsKind` distinguishes unit, tuple and named styles.
`FieldInfo` retains the original position, optional Rust name, type syntax and
ordered attributes. Its `visit_index: Option<usize>` maps the filtered field
iterator to original coordinates and is `None` for `visit(skip)`.
`VariantInfo` retains the original index, name and style. `TypeInfo::Kind` is the
method-free `StructKind` or `EnumKind` marker for the declaration category.
Every raw Rust member remains present, including members with serde skips or
legacy value-presentation controls.

`DeclarationInfo::source` is the effective derive-input declaration rendered as
tokens. Whitespace is normalized; it is not a byte-for-byte source-file copy.
Meaningful literal spelling and doc values are retained. Raw attributes remain
opaque to reflection. Other helper attributes require registration by their
owning derive; `VisitFields` and `VisitVariants` accept `visit` and `serde` helpers.

Static struct fields use `VisitFieldsStaticNamed<V>`; enum variants use
`VisitVariantsStatic<V>` with `VisitVariantFieldsStaticNamed<V>` for their fields.
The consumer implements `Visitor` and ordinary `Visit` support for the markers
it accepts. Consume the returned iterators to perform callbacks:

```rust
fn reflect_record<'a, T, const N: usize, V>(visitor: &mut V)
where
    V: visit_rs::Visitor,
    Record<'a, T, N>: visit_rs::VisitFieldsStaticNamed<V>,
{
    for _ in <Record<'a, T, N> as visit_rs::VisitFieldsStaticNamed<V>>::visit_fields_static_named(
        visitor,
    ) {}
}
```

Borrowed and generic types retain their lifetimes; static traversal does not
require target types to be `'static`.

## Opaque storage and typed metadata

`#[visit(opaque)]` presents storage through `Opaque<T>`, which owns a `Static<T>`
marker. The consumer visits that marker through ordinary `Visit` implementations;
the stored type needs no visiting or `Sync` bound. On a container or variant,
opacity applies to descendant storage fields. Descriptors and attributes remain
complete; selected typed metadata still requires consumer visiting support.

A local `type_attributes` selector declares arbitrary metadata paths containing
Rust types. It parses the **same existing string literal** and emits a
`TypeAttribute<T>` carrier with a `Static<T>` marker in the declaration's lexical
and generic scope:

```rust
#[derive(visit_rs::VisitFields)]
#[visit(opaque, input_wire = "DecoderDto",
        type_attributes(visit::input_wire))]
struct Stored {
    private: Hidden,
}

#[derive(visit_rs::VisitFields)]
struct DecoderDto {
    count: u32,
}

struct Hidden;
```

The consumer supplies `Visit` support for `TypeAttribute<DecoderDto>` and the
opaque storage marker; `Hidden` needs no storage visiting implementation. `input_wire` is caller-authored opaque metadata,
not a reflection policy. Selectors apply only at the container, variant or field
carrying them, not to descendants. Arbitrary nested paths and keywords are
supported; all matching occurrences at that scope are collected. For example, a
field can select unrelated nested metadata in a borrowed generic declaration:

```rust
#[visit(extra(nested(type = "&'a [T; N]")),
        type_attributes(visit::extra::nested::type))]
```

`VisitTypeAttributes<V>::visit_type_attributes` returns a lazy iterator invoking
ordinary `Visit` on `TypeAttribute<T>` carriers. Each carrier owns a `Static<T>`
marker and `TypeAttributeInfo`, including `MetadataPosition` (variant, field,
attribute and occurrence), path, literal tokens and decoded value. Its bounds
are independent of ordinary field traversal. Consumers match coordinates rather
than guessed Rust names.
All selected types require consumer support, even when a consumer ignores a
callback. Unselected attributes remain raw. Reflection supplies no built-in serde
or TypeScript selector list, directional meaning or automatic selector inference.
Typed metadata does not infer arbitrary hook JSON shapes or validate every mixed
conversion role; actual decoders and caller-authored hints own that agreement.

## Features and consumers

Raw `TypeInfo` descriptors, static and selected-metadata traversal capabilities,
and the public metadata API are unconditional. The `meta` feature separately controls metadata on legacy value
reflection descriptors and forwards the matching derive feature.

Serde normalization, TypeScript rendering, scalar bridges and export collections
belong to downstream consumers. For RPC bindings use
[`rpc-toolkit`'s guide](../rpc-toolkit/docs/typescript.md).

## Migration

Derive `VisitFields` for structs and `VisitVariants` for enums to obtain complete
raw type inventory alongside value and static traversal. Legacy value presentation uses explicit `visit` controls
rather than serde renames/skips. Serializer integration requires a consumer-owned
visitor instead of a reflection blanket adapter.

For JSON/TypeScript callers, replace `SerdeShape` with `VisitFields` for structs
or `VisitVariants` for enums and use RPC's
`reflect_ts!`, `ts_export!` and `impl_ts_shape!` bridges. Move renderer imports and
integration features to RPC. Select existing type-valued conversion/wire literals
locally and mark hidden storage opaque; see
[the consumer migration guide](../rpc-toolkit/docs/typescript.md#migration-from-the-ts-rs-feature).

## Development

Run `cargo test -p visit-rs` for neutral reflection tests and
`make rpc-toolkit-test` for reflection/RPC feature matrices and binding
interoperability. See [AGENTS.md](AGENTS.md) and [ARCHITECTURE.md](ARCHITECTURE.md).

## Provenance and license

Migrated from [dr-bonez/visit-rs](https://github.com/dr-bonez/visit-rs), including the
metadata/generic-enum work from [PR #1](https://github.com/dr-bonez/visit-rs/pull/1).
Maintained here under MIT; see [LICENSE](LICENSE).
