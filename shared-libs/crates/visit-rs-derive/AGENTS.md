# AGENTS.md — visit-rs-derive

Read root and `shared-libs/` guidance first. This proc-macro crate belongs to the
`visit-rs` runtime. Its feature flags must match the runtime's generated members.

`VisitFields` for structs and `VisitVariants` for enums generate value/static
traversal, unconditional `TypeInfo` facts and independent
`VisitTypeAttributes<V>` implementations. Keep source/descriptor emission under
one raw-facts collector. `TypeInfo::Kind` uses method-free `StructKind` / `EnumKind`
markers; consumers own dispatch. `FieldInfo::visit_index` maps filtered field
callbacks to original coordinates. Retain all original Rust
members, indices, names, styles and ordered opaque attributes; normalize source
whitespace without changing meaningful literal/doc values.

Only explicit generic `visit` controls affect callback selection. Container,
variant and field opacity removes storage visiting bounds, not descriptors or
selected metadata callbacks. Local `type_attributes` selectors accept arbitrary
paths and resolve the same existing string literal in its owner/generic scope.
Do not introduce built-in serde/TS selectors, skip/rename/default/layout policy,
semantic derives or TypeScript proc macros. Legacy value `visit(skip/rename)`
controls remain separate from the complete raw inventory.

Run `make rpc-toolkit-test` and `make start-core-format` from the root. Neutral
macro regressions belong in runtime reflection tests; domain fixtures belong in
RPC. Align legacy `meta` initializers with the runtime feature. Tuple value
visitors preserve original field indices and contiguous iterator positions.
