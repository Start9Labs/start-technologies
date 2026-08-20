# json-patch

[JSON Patch (RFC 6902)](https://tools.ietf.org/html/rfc6902) and
[JSON Merge Patch (RFC 7396)](https://tools.ietf.org/html/rfc7396) for the `patch-db` stack.

Build and test with `cargo … -p json-patch`. The `diff` feature (on by default) adds `json_patch::diff`,
which computes a patch between two documents. See the crate-level rustdoc in `src/lib.rs` for usage.

## Provenance

This crate is a fork of [idubrov/json-patch](https://github.com/idubrov/json-patch), originally written by
Ivan Dubrov. It has diverged substantially: `serde_json` has been replaced throughout by the sibling
[`imbl-value`](../../imbl-value) and [`json-ptr`](../json-ptr) crates, so patches apply to the persistent,
cheaply-cloned `imbl_value::Value` that `patch-db` is built on. It is maintained here as part of the
monorepo, not tracked against upstream.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option — inherited from upstream. Original work Copyright (c) Ivan Dubrov; modifications
Copyright (c) Start9 Labs, Inc.
