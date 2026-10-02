# AGENTS.md — visit-rs-derive

Read root and `shared-libs/` guidance first. This proc-macro crate belongs to the
`visit-rs` runtime. Its feature flags must match the runtime's generated members.

- `lib.rs` generates struct reflection; `enum_variants.rs` generates enum reflection.
- `helpers.rs` owns reflection field selection and naming; use serde's rename rule
  primitive rather than copying its case-conversion algorithms.
- `attrs.rs` owns raw attribute parsing and feature-gated metadata initializers.
- `shape.rs` owns serde JSON-shape lowering through `serde_derive_internals`.

The serde-internals dependency is exactly pinned because its public AST API is
unstable. Upgrade it together with syn and exercise actual serde serialization and
deserialization fixtures, not only macro-token assertions.

Run `make rpc-toolkit-test` and `make start-core-format` from the root. Macro
regressions belong in runtime reflection tests or the RPC interoperability suite.
Skipped fields must not impose visiting bounds, and tuple visitors must preserve
original field indices while using contiguous iterator positions.
