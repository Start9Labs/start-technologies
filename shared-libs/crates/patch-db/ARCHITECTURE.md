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

The Rust side owns the persistent state and produces patches. The TypeScript side consumes those patches and maintains a local mirror for reactive UI bindings. They are separate implementations of the same concepts (not WASM/FFI) — compatibility is maintained through shared RFC 6901/6902 semantics.

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

| Type                 | Role                                                                                                                              |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `PatchDb`            | Thread-safe async handle (clone to share). All reads/writes go through this.                                                      |
| `TypedPatchDb<T>`    | Generic wrapper that enforces a schema type `T` via `HasModel`.                                                                   |
| `Store`              | Internal state container. File-backed with CBOR. Holds the current `Value`, revision counter, and `Broadcast`.                    |
| `Dump`               | Snapshot: `{ id: u64, value: Value }`                                                                                             |
| `Revision`           | Incremental change: `{ id: u64, patch: DiffPatch }`                                                                               |
| `DiffPatch`          | Newtype over `json_patch::Patch` with scoping, rebasing, and key-tracking methods.                                                |
| `DbWatch`            | Combines a `Dump` + `Subscriber` into a `Stream` of values.                                                                       |
| `TypedDbWatch<T>`    | Type-safe wrapper around `DbWatch`.                                                                                               |
| `Subscriber`         | `tokio::sync::mpsc::UnboundedReceiver<Revision>`.                                                                                 |
| `Broadcast`          | Fan-out dispatcher. Holds `ScopedSender`s that filter patches by JSON Pointer prefix. Automatically removes disconnected senders. |
| `MutateResult<T, E>` | Pairs a `Result<T, E>` with an optional `Revision`, allowing callers to check both the outcome and whether a patch was produced.  |

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
  ├─ Serialize patch as CBOR, append to file
  ├─ Compress (rewrite snapshot) every 4096 revisions
  └─ Broadcast::send(Revision)
       └─ For each ScopedSender: scope patch to pointer, send if non-empty
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
  └─ Dump?     → replace cache, update all watchedNodes
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
- A revision triggers updates only for watchers whose path overlaps with any operation in the patch (prefix match in either direction)
- A dump triggers updates for all watchers

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

The on-disk format is a sequence of CBOR values:

```
[ revision: u64 ] [ value: Value ] [ patch₁ ] [ patch₂ ] ... [ patchₙ ]
```

- On open, the file is read sequentially: revision counter, then root value, then patches are replayed
- On write, new patches are appended as CBOR
- Every 4096 revisions, the file is compacted: a fresh snapshot is written atomically via a `.bak` temp file
- A `.failed` file logs patches that couldn't be applied (data recovery aid)

## Concurrency model

- `PatchDb` wraps `Arc<RwLock<Store>>` — multiple concurrent readers, exclusive writer
- `Broadcast` uses `mpsc::unbounded_channel` per subscriber — writes never block on slow consumers
- `OPEN_STORES` static mutex prevents the same file from being opened twice in the same process
- `FdLock` provides OS-level file locking for cross-process safety
