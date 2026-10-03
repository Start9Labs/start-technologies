# yasi Architecture

`yasi` is a string interner that deduplicates strings behind `Arc`, hashing and comparing through
`Display` so any `Display`-able value can be looked up against the table without first allocating a
`String`. Small strings are kept inline on the stack; equal heap strings collapse to a single
shared `Arc`; well-known constants can be registered as `'static`.

## Place in the monorepo

- **Path:** `shared-libs/crates/yasi`
- **Package name:** `yasi` (library name `yasi`) — build/test with `cargo … -p yasi`.
- **Crate type:** library (`src/lib.rs`).
- **Consumers (first-party, direct path deps — no `[patch]`):**
  - `exver` — `yasi = { path = "../yasi" }`
  - `imbl-value` — `yasi = { path = "../yasi", features = ["serde"] }`, and re-exports the
    `ts` feature through its own `ts` feature.
- **Dependencies:** `hashbrown` (`raw` API), `lazy_static`, `tinyvec`, `xxhash-rust` (xxh3);
  `serde` and `visit-rs` are optional.

## How it works

### The three representations

`InternedString` wraps a private `StringRepr`:

- `Stack(ArrayVec<[u8; 20]>)` — strings up to `STACK_STR_SIZE` (20) bytes, inlined. These never
  touch the global table, so they cost no allocation and no lock, but they are also not
  deduplicated across instances.
- `Heap(Arc<TableString>)` — longer strings. Equal values share one `Arc`; the global table holds
  a `Weak` to it.
- `Static(&'static str)` — a borrowed compile-time string, registered without copying.

`StringRepr` equality fast-paths pointer/`Arc` identity and falls back to byte comparison; ordering
defers to the underlying `&str` when not pointer-equal.

### Display-based hashing and equality

Interning never requires the caller to materialize a `String` up front. `DisplayHasher` implements
`std::fmt::Write`: it feeds the `Display` output into an xxh3 hasher and, simultaneously, tries to
buffer the first ≤20 bytes into an `ArrayVec`. If the rendered value fits, the result is a
`Stack` variant with no table interaction; if it overflows, the stack buffer is dropped and only
the hash survives. `DisplayEq` likewise streams a candidate's `Display` output and compares it
byte-for-byte against a target `&str`, short-circuiting on the first mismatch — so a table hit is
confirmed without allocating the candidate.

### The global table

A single `lazy_static` `TABLE: RwLock<RawTable<StringRef>>` (hashbrown's raw API) holds one entry
per interned heap/static string. Entries are `StringRef::Heap(Weak<TableString>)` or
`StringRef::Static(&'static str)`. `intern` does a read-locked lookup first, then upgrades to a
write lock and re-checks (guarding the insert race) before inserting a new `Weak`.

`TableString` owns the heap `String` and, on `Drop`, re-acquires the write lock to erase its own
entry (matching on a `Weak` whose `strong_count` has reached zero). This eviction-on-drop is what
keeps the table from leaking dead weaks — and is the source of the crate's one subtle hazard.

### Re-entrancy and the keepalive discipline

Because `TableString::drop` takes `TABLE.write()`, dropping the last `Arc<TableString>` while a
table guard is still held would re-enter `std::sync::RwLock` and self-deadlock. Inside `intern`
and `intern_static`, every `Arc` obtained from `Weak::upgrade` in an `eq`/rehash closure is stashed
in a `RefCell<Vec<Arc<TableString>>>` keepalive (the `RefCell` is needed because hashbrown's rehash
closure is `Fn`). Each lock guard is explicitly `drop`ped, and only afterward is the keepalive
`Vec` released — so any last-ref drop fires with no guard held. The `intern_drop_race_does_not_deadlock`
regression test reproduces the original wedge (a 0.4.0-beta.9 `startd`) with many threads tight-
looping `intern → drop` over a few long keys, and fails via a watchdog channel rather than hanging.

### Public API surface

- `InternedString` — the main type.
  - `intern<S: Display + Into<String>>(s)` — intern (stack-inline, table-dedup, or insert).
  - `from_display<S: Display + ?Sized>(&s)` — intern a `Display`-only value (no `Into<String>`).
  - `from_static(&'static str)` — wrap a static string without touching the table.
  - `intern_static(&'static str)` — register a static into the table, upgrading any existing heap
    entry to the `Static` variant.
  - Trait impls: `Deref<Target = str>`; `AsRef<[u8] | OsStr | Path | str>`; `Borrow<str>`
    (also for `&InternedString`); `Clone`, `Debug`, `Default`, `Display`; `Eq`, `Hash`, `Ord`;
    `From<T: Display + Into<String>>`, `FromStr` (`Err = Infallible`); and
    `PartialEq<&str> / PartialEq<str> / PartialEq<Cow<str>>`.
- `TableHasher` — public low-level xxh3 wrapper used internally; rarely needed by consumers.
- With `serde`: `Serialize` (as a plain string) and `Deserialize` (interns any string/bytes form).
- With `ts`: `visit_rs::TS` reports the wire type as `string` in both directions.

## Further reading

- [README.md](README.md) — what it is and how to use it.
- [AGENTS.md](AGENTS.md) — contributor workflow.
- [../../ARCHITECTURE.md](../../ARCHITECTURE.md) — the `shared-libs` overview.
