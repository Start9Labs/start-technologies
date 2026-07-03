# AGENTS.md — yasi

`yasi` ("Yet Another String Interner") is a small first-party Rust library in the start-os
monorepo at `shared-libs/crates/yasi`. It deduplicates strings behind `Arc`, computing `Hash`
and `Eq` through `Display` so any `Display` value can be interned without allocating unless it's
new. `CLAUDE.md` is a one-line `@AGENTS.md` import; edit this file, not that one. See
[ARCHITECTURE.md](ARCHITECTURE.md) for internals and [CONTRIBUTING.md](CONTRIBUTING.md) for the
contributor workflow.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/lib.rs` — the whole library. Defines the public `InternedString` and `TableHasher`; the
  internal `StringRepr` enum (`Heap`/`Stack`/`Static`), `StringRef` table entry, `DisplayHasher`
  and `DisplayEq` (Display-based hashing/equality), `TableString` (the heap-owned string with the
  table-eviction `Drop`), and the `lazy_static` global `TABLE: RwLock<RawTable<StringRef>>`.
  Holds the two unit tests, including the `intern → drop` deadlock regression test.
- `src/serde.rs` — feature-gated (`serde`): `Serialize`/`Deserialize` for `InternedString` via an
  `InternedStringVisitor` (deserializes any string/bytes form by interning it).
- `src/ts_rs.rs` — feature-gated (`ts-rs`): `ts_rs::TS` impl that reports the type as `"string"`,
  plus a test that exports a struct embedding `InternedString`.

## Build & test (run from the repo root)

```bash
cargo build -p yasi                  # build with default (no) features
cargo build -p yasi --all-features   # build with serde + ts-rs
cargo test  -p yasi                  # run unit tests (incl. the deadlock regression test)
cargo test  -p yasi --all-features   # also run the serde / ts-rs paths
make start-core-format                     # format the shared Rust crates (incl. yasi)
make start-core-format-check               # CI read-only format check
```

## Gotchas

- `Cargo.toml` declares `edition = "2024"` and the code uses 2024 features (let-chains in `eq`
  closures). Build with a toolchain that supports edition 2024; sibling `exver` is still on 2021.
- Features `serde` and `ts-rs` are **not** in `default`. Consumers opt in (e.g. `imbl-value`
  enables `serde` and forwards `ts-rs`); plain `cargo test -p yasi` exercises neither module.
- Strings of 20 bytes or fewer (`STACK_STR_SIZE`) are inlined in `StringRepr::Stack` and never
  touch the global `TABLE` — they are not deduplicated and incur no lock. The regression test
  deliberately uses keys longer than 20 bytes to force the table path.
- The global `TABLE` is an `RwLock<RawTable<StringRef>>`. `TableString::drop` re-acquires
  `TABLE.write()` to evict its entry, so any `Arc<TableString>` obtained via `Weak::upgrade`
  inside an `eq`/rehash closure must be parked in a `keepalive` `Vec` and the guard explicitly
  dropped **before** that `Vec` — otherwise a last-strong-ref drop re-enters the lock and
  `std::sync::RwLock` self-deadlocks (the bug `intern_drop_race_does_not_deadlock` guards). The
  `RefCell` around `keepalive` exists because hashbrown's rehash closure is `Fn`, not `FnMut`.
