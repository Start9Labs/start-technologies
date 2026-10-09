# visit-rs architecture

The runtime owns generic value/type traversal and opaque metadata. Its sibling
`visit-rs-derive` generates implementations; neither depends on RPC, serde policy
or TypeScript facilities.

`Static<T>`, `Covered<T>` and `Named<T>` select a type marker, covered value or named
field. Visitors determine results and field handling. Returned iterators/streams
preserve lazy traversal. Legacy `visit(skip/rename)` controls value presentation;
serde skips and renames do not change reflection.

`reflection` owns `TypeInfo`, method-free `StructKind` / `EnumKind` markers and
`Opaque<T>` / `TypeAttribute<T>` carriers. The runtime root owns the independent
`VisitTypeAttributes<V>` trait. `VisitFields` and `VisitVariants` derive declaration
facts and traversal implementations through one raw-facts collector. `DeclarationInfo`
retains all original Rust fields, variants, names, styles, generic parameters,
source tokens and ordered attributes. Raw descriptors and the public metadata API
are unconditional; `meta` separately gates legacy value descriptor metadata.
Source tokens have normalized whitespace, with meaningful literal/doc values
preserved, rather than exact source-file byte fidelity.

Static field callbacks use ordinary `Visit` implementations for `Static<T>`
and named markers. `FieldInfo::visit_index` maps filtered callbacks to original
coordinates. Explicit `visit(opaque)` at a container, variant or field presents
storage as `Opaque<T>` owning `Static<T>`, without stored-type visiting or `Sync`
bounds or loss of raw descriptors. Local `visit(type_attributes(...))` selectors
collect arbitrary metadata paths from existing string literals. Selectors are
local, non-inherited and free of domain meaning. `VisitTypeAttributes<V>` invokes
ordinary `Visit` on compiler-resolved `TypeAttribute<T>` carriers in the original
owner scope; its selected-type bounds are independent of field traversal.
Selected types still require consumer visiting support.

Consumers own policy, scalar implementations, instantiated declaration identity,
collisions and recursion termination. Borrowed reflection imposes no target
`'static` requirement. The RPC consumer reserves pending identities before descent
and gives recursive generic root bridges payload bounds rather than recursive
whole-root traversal bounds. See [the RPC architecture](../rpc-toolkit/ARCHITECTURE.md)
for JSON normalization and TypeScript ownership.
