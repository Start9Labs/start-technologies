# visit-rs-derive

Proc macros re-exported by the sibling `visit-rs` runtime:

- `VisitFields` — struct reflection.
- `VisitVariants` — enum reflection.
- `SerdeShape` — directional serde JSON-shape traversal.
- `TS` — TypeScript declarations and annotation-owned concrete exports.

`TS` and `SerdeShape` share serde-internals lowering. Directional
`visit(input_wire = "RustType")` / `visit(output_wire = "RustType")` overrides
reuse actual custom serializer/deserializer representations. See the runtime's
TypeScript section for declaration naming, export namespaces and generic roots.

Use them through `visit_rs`; direct consumers must align the `meta` feature with
the runtime. See [the runtime guide](../visit-rs/README.md),
[AGENTS.md](AGENTS.md) and [ARCHITECTURE.md](ARCHITECTURE.md).

Migrated with [visit-rs PR #1](https://github.com/dr-bonez/visit-rs/pull/1). MIT;
see [LICENSE](LICENSE).
