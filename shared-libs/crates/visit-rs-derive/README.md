# visit-rs-derive

Generic proc macros re-exported by the sibling `visit-rs` runtime:

- `VisitFields` — struct value/static traversal and declaration facts.
- `VisitVariants` — enum value/static traversal and declaration facts.

Both derives implement unconditional `TypeInfo` with method-free `StructKind` /
`EnumKind` markers and independent `VisitTypeAttributes<V>` traversal.
Descriptors retain all original Rust members, names, styles, source tokens and
ordered opaque attributes. Source whitespace is normalized while meaningful
literal/doc values are preserved. Raw metadata is unconditional; legacy value
metadata remains gated by `meta`.

`visit(opaque)` presents storage through ordinary `Opaque<T>` marker callbacks
at a container, variant or field, without stored-type visiting or `Sync` bounds. Local `visit(type_attributes(...))` selects arbitrary type-valued metadata
paths from their existing string literals. The compiler resolves those types in
the owner scope; ordinary `Visit` implementations consume `TypeAttribute<T>`
carriers independently of field traversal. Every selected type requires consumer
visiting support.
Serde/TypeScript interpretation and consumer bridge macros belong downstream.

Use these derives through `visit_rs`; direct consumers must align `meta` with the
runtime. See [the runtime guide](../visit-rs/README.md) for examples,
[AGENTS.md](AGENTS.md) and [ARCHITECTURE.md](ARCHITECTURE.md).

Migrated with [visit-rs PR #1](https://github.com/dr-bonez/visit-rs/pull/1). MIT;
see [LICENSE](LICENSE).
