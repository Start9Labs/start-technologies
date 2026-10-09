# AGENTS.md — visit-rs

Read root and `shared-libs/` guidance first. The runtime is used by start-core,
StartWRT and rpc-toolkit's opt-in binding generator. Its proc macros live in the
sibling `visit-rs-derive` crate.

`src/lib.rs` owns generic value traversal, wrappers and independent
`VisitTypeAttributes<V>` traversal; `reflection` owns raw `TypeInfo` facts,
method-free kind markers and `Opaque<T>` / `TypeAttribute<T>` carriers;
`metadata.rs` owns opaque attribute metadata. Preserve original Rust members, names, styles and ordered
attributes. Keep serde interpretation, serializer adapters, TypeScript facilities,
scalar bridges and export registration in consumers.

Explicit `visit(opaque)` avoids storage visiting bounds without erasing metadata.
Local `type_attributes` selectors resolve existing literals in the owner's scope;
they must remain arbitrary, non-inherited and free of built-in domain policy.
Selected metadata types still require consumer visiting support. Raw reflection
is unconditional; `meta` controls legacy value descriptors separately.

Run `cargo test -p visit-rs` and `cargo test -p visit-rs --no-default-features` for
neutral reflection coverage. `make rpc-toolkit-test` runs feature matrices and
consumer interoperability. Format with `make start-core-format`. Check start-core
and StartWRT after changing trait signatures or macro output. Align `meta` with
the derive feature for legacy descriptor initializers.
