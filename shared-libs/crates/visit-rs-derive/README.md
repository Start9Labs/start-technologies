# visit-rs-derive

Proc macros re-exported by the sibling `visit-rs` runtime:

- `VisitFields` — struct reflection.
- `VisitVariants` — enum reflection.
- `SerdeShape` — directional serde JSON-shape traversal.

Use them through `visit_rs`; direct consumers must align the `meta` feature with
the runtime. See [the runtime guide](../visit-rs/README.md),
[AGENTS.md](AGENTS.md) and [ARCHITECTURE.md](ARCHITECTURE.md).

Migrated with [visit-rs PR #1](https://github.com/dr-bonez/visit-rs/pull/1). MIT;
see [LICENSE](LICENSE).
