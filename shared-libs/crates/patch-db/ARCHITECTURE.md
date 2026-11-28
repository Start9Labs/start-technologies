# Architecture

## High-level design

patch-db is split into two layers that communicate over a transport boundary:

```
┌─────────────────────────────────────────────┐
│              TypeScript Client              │
│  PatchDB<T> ─ RxJS observables ─ watch$()  │
│         ▲                                   │
│         │  Update<T>[] (Dump | Revision)    │
│         │  over WebSocket / SSE / etc.      │
└─────────┼───────────────────────────────────┘
          │
┌─────────┼───────────────────────────────────┐
│         ▼                                   │
│              Rust Backend                   │
│  PatchDb ─ Store ─ Broadcast ─ Subscriber  │
│                                             │
│  ┌──────────┐  ┌───────────┐  ┌────────────┐ │
│  │ json-ptr │  │json-patch │  │ serde_cbor │  │
│  │ RFC 6901 │  │ RFC 6902  │  │  storage   │  │
│  └──────────┘  └───────────┘  └────────────┘  │
└─────────────────────────────────────────────┘
```

The Rust side owns the database state and produces patches. The TypeScript side consumes those patches and maintains a local mirror for reactive UI bindings. They are separate implementations of the same concepts (not WASM/FFI) — compatibility is maintained through shared RFC 6901/6902 semantics.

## Project structure

```
patch-db/
├── core/                          # Core Rust crate — PatchDb, Store, typed wrappers
├── macro/                         # Procedural macro crate (derives HasModel)
├── macro-internals/               # Macro implementation details
├── util/                          # CLI tool (dump/load database files)
├── json-ptr/                      # RFC 6901 JSON Pointer implementation
├── json-patch/                    # RFC 6902 JSON Patch implementation
└── client/                        # TypeScript client library (RxJS-based)
    └── lib/
        ├── patch-db.ts            # PatchDB<T> class
        ├── json-patch-lib.ts      # Client-side patch application
        └── types.ts               # Revision, Dump, Update, PatchOp
```

## Rust crates

### `core` (crate name: `patch-db`)

The main database engine. Key types:

| Type                 | Role                                                                                                                                                                                                                                                                                      |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PatchDb`            | Thread-safe async handle (clone to share). All reads/writes go through this.                                                                                                                                                                                                              |
| `TypedPatchDb<T>`    | Generic wrapper that enforces a schema type `T` via `HasModel`.                                                                                                                                                                                                                           |
| `Store`              | Internal state container. Holds the current `Value`, revision counter, `Broadcast`, and optional CBOR file storage.                                                                                                                                                                       |
| `Dump`               | Snapshot: `{ id: u64, value: Value }`                                                                                                                                                                                                                                                     |
| `Revision`           | Incremental change: `{ id: u64, patch: DiffPatch }`                                                                                                                                                                                                                                       |
| `DiffPatch`          | Newtype over `json_patch::Patch` with scoping, rebasing, and key-tracking methods.                                                                                                                                                                                                        |
| `DbWatch`            | Combines a `Dump` + `Subscriber` into a `Stream` of values.                                                                                                                                                                                                                               |
| `TypedDbWatch<T>`    | Type-safe wrapper around `DbWatch`.                                                                                                                                                                                                                                                       |
| `Subscriber`         | `tokio::sync::mpsc::UnboundedReceiver<Revision>`.                                                                                                                                                                                                                                         |
| `JsonSubscriber`     | Bounded JSON revision receiver. Closes permanently on overflow.                                                                                                                                                                                                                           |
| `Broadcast`          | Per-pointer fan-out with shared JSON serialization and cached dumps. Reclaims dropped typed senders on send and typed subscription. Retains the last 1024 revisions under a random per-open epoch.                                                                                        |
| `Catchup`            | What a resuming subscriber lacks: a `Dump`, or the scoped revisions since its cursor plus the current revision id.                                                                                                                                                                        |
| `Projection`         | A union of selectors, each with an inclusion, exclusions and depth. An empty union is identity. Projected arrays keep their length with null placeholders, and an array a revision shifts is sent whole unless kept whole; dumps, revisions and extensions use the same projection owner. |
| `MutateResult<T, E>` | Pairs a `Result<T, E>` with an optional `Revision`, allowing callers to check both the outcome and whether a patch was produced.                                                                                                                                                          |

`Selector` owns one included subtree and its exclusions/depth. `ProjectedSubscriber` owns a mutable subscription id and an ordered `Update` receiver. `Update::Revision` carries normal projected changes; `Update::Extend` carries fields added by a selection change, or an empty acknowledgement when nothing is added. Mutable and fixed typed subscription admission reclaim closed senders without requiring a database revision; reprojection also retires empty listener entries.

#### Write path

```
caller
  │
  ▼
PatchDb::put / apply / apply_function / mutate
  │
  ▼
Store::apply(DiffPatch)
  ├─ Apply patch in-memory (with undo on failure)
  ├─ For file-backed stores: serialize patch as CBOR, append to file
  ├─ For file-backed stores: compress (rewrite snapshot) every 4096 revisions
  └─ Broadcast::send(Revision)
       ├─ Append to the bounded revision history
       ├─ Scope and project each patch once per subscribed pointer and projection
       ├─ Deliver typed revisions to unbounded subscribers
       └─ Serialize once per pointer for shared bounded JSON broadcast rings
```

#### Read path

```
caller
  │
  ▼
PatchDb::dump / get / exists / keys
  │
  ▼
Store (RwLock read guard)
  └─ Navigate Value via JsonPointer
```

#### Subscription path

```
PatchDb::subscribe(ptr)    → Subscriber (mpsc receiver)
PatchDb::watch(ptr)        → DbWatch (Dump + Subscriber, implements Stream)
PatchDb::dump_and_sub(ptr) → (Dump, Subscriber)
PatchDb::catch_up_and_sub(ptr, since) → (epoch, Catchup, Subscriber)
PatchDb::catch_up_and_sub_projected(ptr, projection, since) → (epoch, Catchup, Subscriber)
PatchDb::dump_projected(ptr, projection) → Dump
PatchDb::catch_up_and_sub_mutable(ptr, projection, since) → (epoch, Catchup, ProjectedSubscriber)
PatchDb::reproject(id, projection) → bool
PatchDb::catch_up_and_sub_json(ptr, since, capacity) → Result<(epoch, JsonCatchup, JsonSubscriber)>
PatchDb::dump_and_sub_json(ptr, capacity) → Result<(Bytes, JsonSubscriber)>
```

### `macro` / `macro-internals`

Procedural macro that derives `HasModel` for structs and enums:

```rust
#[derive(HasModel)]
#[model = "Model<Self>"]
struct Config {
    hostname: String,
    port: u16,
}
```

Generates:

- `impl HasModel for Config { type Model = Model<Self>; }`
- Typed accessor methods: `as_hostname()`, `as_hostname_mut()`, `into_hostname()`
- `from_parts()` constructor
- `destructure_mut()` for simultaneous mutable access to multiple fields
- Respects `serde(rename_all)`, `serde(rename)`, `serde(flatten)`
- Enum support with `serde(tag)` / `serde(content)` encoding

### `json-ptr`

RFC 6901 JSON Pointer implementation. Provides:

- `JsonPointer<S, V>` — generic over string storage and segment list representation
- Zero-copy `BorrowedSegList` for efficient path slicing
- Navigation: `get`, `get_mut`, `set`, `insert`, `remove`, `take`
- Path algebra: `starts_with`, `strip_prefix`, `common_prefix`, `join_end`, `append`
- `ROOT` constant for the empty pointer

### `json-patch`

RFC 6902 JSON Patch implementation. Provides:

- `Patch(Vec<PatchOperation>)` — the patch type
- `PatchOperation` enum: `Add`, `Remove`, `Replace`, `Test`, `Move`, `Copy`
- `patch()` — apply a patch to a `Value`, returns an `Undo` for rollback
- `diff()` — compute the minimal patch between two `Value`s

### `util`

CLI tool with two subcommands:

- `dump <path>` — deserialize a patch-db file and print the final state as JSON
- `from-dump <path>` — read JSON from stdin and write it as a fresh patch-db file

## TypeScript client

### `PatchDB<T>`

RxJS-based observable database client. Consumes `Update<T>[]` from a transport source.

**Data flow:**

```
source$ (Observable<Update<T>[]>)
  │
  ▼
PatchDB.processUpdates()
  ├─ Revision? → applyOperation() for each op, then update matching watchedNodes
  └─ Dump?     → reconcile cache, update changed watchedNodes
  │
  ▼
cache$ (BehaviorSubject<Dump<T>>)
  │
  ▼
watch$(...path) → BehaviorSubject per unique path → Observable to consumer
```

**Key design decisions:**

- `watch$()` has overloads for 0–6 path segments, providing type-safe deep property access
- Watched nodes are keyed by their JSON Pointer path string
- A revision considers watchers whose path overlaps with any operation in the patch (prefix match in either direction)
- A dump reconciles equal subtrees and considers every watcher
- Watches emit subsequent values only when `value !== previousValue`

### `SubtreePatchDB<T>`

One stream carries the union of reference-counted watches, with a five-second release delay and 250ms batching. Selection changes prune the browser replica immediately and request an ordered server extension. Until that extension arrives, revisions apply to a retained pre-change replica before projecting into the browser replica; selected array positions stay correct through in-flight insertions and removals. Watches acquired synchronously during a revision flush join the next serialized selection update. Confirmed coverage retains provable subsets of the previous selection; no coverage is `NOTHING`, while `[]` means the whole subtree. New watches wait for coverage confirmation. Failure resumes the confirmed selection from its single epoch/revision cursor rather than replaying discarded fields. Every update, dumps included, keeps the identity of each subtree it leaves unchanged in the replica and in projected watch values, and a watch emits only when its value is a new object.

`Projection::extension` computes fields newly required by a selection against the current store value. `PatchDb::reproject` changes the listener and sends the extension under the store lock, before later revisions. The TypeScript projection module mirrors selector unions and extension semantics.

### `json-patch-lib`

Client-side RFC 6902 implementation (add/remove/replace only — no test/move/copy since those aren't produced by the server's `diff()`).

Operations are applied immutably — objects are spread-copied, arrays are sliced — to play nicely with change detection in UI frameworks.

### Types

| Type           | Definition                                                  |
| -------------- | ----------------------------------------------------------- |
| `Revision`     | `{ id: number, patch: Operation<unknown>[] }`               |
| `Dump<T>`      | `{ id: number, value: T }`                                  |
| `Update<T>`    | `Revision \| Dump<T>`                                       |
| `PatchOp`      | Enum: `'add' \| 'remove' \| 'replace'`                      |
| `Operation<T>` | `AddOperation<T> \| RemoveOperation \| ReplaceOperation<T>` |

## Storage format

`PatchDb::in_memory()` creates an independent database with a null root and
revision zero. It shares the read, mutation, and broadcast paths with file-backed
databases, without file storage or file locks. Cloned handles share the store;
its state is discarded when the last handle drops.

`PatchDb::open(path)` creates or opens a file-backed database. The on-disk format
is a sequence of CBOR values:

```
[ revision: u64 ] [ value: Value ] [ patch₁ ] [ patch₂ ] ... [ patchₙ ]
```

- On open, the file is read sequentially: revision counter, then root value, then patches are replayed
- On write, new patches are appended as CBOR
- Every 4096 revisions, the file is compacted: a fresh snapshot is written atomically via a `.bak` temp file
- A `.failed` file logs patches that couldn't be applied (data recovery aid)

## Concurrency model

- `PatchDb` wraps `Arc<RwLock<Store>>` — multiple concurrent readers, exclusive writer.
- `pause_mutations` waits for value mutations and holds their shared acquisition gate ahead of the Store write lock. Cloned/typed value mutations and final file-backed close wait; cached reads and subscription registration continue. Callers must not await a mutation or close while retaining the pause. This separation permits persistence to be held unavailable without a queued persistence writer blocking readers.
- Typed subscriptions use `mpsc::unbounded_channel`; bounded JSON subscriptions share Tokio broadcast rings grouped by pointer and capacity. Writes never block on slow consumers.
- JSON overflow closes that receiver permanently; a fresh atomic dump/subscription recovers state. Other receivers remain independent.
- JSON dumps are cached per pointer until any database revision. Scoped revisions serialize once per pointer and share immutable `Bytes` across all capacities and listeners. Dropped JSON receivers unregister from their ring without waiting for a database change.
- `OPEN_STORES` static mutex prevents the same file from being opened twice in the same process
- `FdLock` provides OS-level file locking for cross-process safety
