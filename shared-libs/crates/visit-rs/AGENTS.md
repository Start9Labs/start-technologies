# AGENTS.md — visit-rs

Read root and `shared-libs/` guidance first. The runtime is used by start-core,
StartWRT and rpc-toolkit's opt-in binding generator. Its proc macros live in the
sibling `visit-rs-derive` crate.

`src/lib.rs` owns reflection contracts and wrappers; `metadata.rs` owns raw
attribute metadata; `shape.rs` owns directional JSON-shape traversal contracts;
`serde.rs` makes serde serializers into value visitors. Keep value reflection and
input/output JSON shape semantics separate.

Run `make rpc-toolkit-test` from the root for both crate feature matrices and the
consumer interoperability tests. `cargo test -p visit-rs` runs this crate's tests;
`cargo test -p visit-rs --no-default-features` verifies metadata-free derives. Format
with `make start-core-format`. Check start-core and StartWRT after changing trait
signatures or macro output. The `meta` feature must remain aligned with the proc
macro feature so generated initializers match the runtime types.
