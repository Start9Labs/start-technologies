# visit-rs-derive architecture

The proc macros generate generic runtime traversal without constructing values
for static reflection. `VisitFields` for structs and `VisitVariants` for enums
emit value/static traversal, unconditional `TypeInfo` facts and independent
`VisitTypeAttributes<V>` traversal through one raw declaration owner.
`TypeInfo::Kind` identifies structs and enums with method-free markers;
consumers own traversal dispatch.

Descriptors retain every original member, name, index, unit/tuple/named style,
generic parameter, ordered attribute and effective declaration token stream.
Source whitespace is normalized; meaningful literal spelling and doc values are
preserved. Raw reflection is unconditional, while `meta` gates legacy value
metadata initializers separately. Serde helper registration exposes attributes;
it supplies no serde understanding.

`FieldInfo::visit_index` maps filtered field callbacks to original coordinates.
Explicit `visit(opaque)` presents storage through `Opaque<T>` owning `Static<T>`;
the stored type needs no visiting or `Sync` bound. Local
`visit(type_attributes(...))` selectors parse arbitrary selected metadata strings
as Rust types and emit `TypeAttribute<T>` carriers with `Static<T>` markers in
the owner scope. Independent `VisitTypeAttributes<V>` traversal invokes ordinary
`Visit` on those carriers. Selected-type bounds do not constrain field traversal. Selectors carry no direction, conversion,
hook or JSON meaning and are not inherited.

Serde normalization, TypeScript rendering and declarative consumer bridges live
in rpc-toolkit. Neutral derive tests cover raw fidelity, generic/borrowed types
and opacity; RPC tests own serde fixtures and TypeScript interoperability.
