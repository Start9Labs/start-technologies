# AGENTS.md — patch-db

patch-db is a JSON Patch–based database with a Rust backend and a TypeScript client. Its six Rust crates are first-party members of the start-os root Cargo workspace — `start-core` consumes the Rust `core` via a path dep, and the web front ends consume the TS `client`. `CLAUDE.md` is a one-line `@AGENTS.md` import. See [README.md](README.md) for what it is and quick-start examples, [ARCHITECTURE.md](ARCHITECTURE.md) for design/storage/concurrency, and [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

| Area          | Path                                                                            |
| ------------- | ------------------------------------------------------------------------------- |
| Core API      | `core/src/store.rs` — `PatchDb`, `TypedPatchDb`, `Store`, `MutateResult`        |
| Types         | `core/src/patch.rs` — `Revision`, `Dump`, `DiffPatch`, `diff()`                 |
| Subscriptions | `core/src/subscriber.rs` — `DbWatch`, `TypedDbWatch`, `Subscriber`, `Broadcast` |
| Model system  | `core/src/model.rs` — `HasModel`, `Model`, `ModelExt`, `Pointer`                |
| Derive macro  | `macro-internals/src/lib.rs` — `HasModel` derive implementation                 |
| Error types   | `core/src/lib.rs` — `Error` enum, re-exports                                    |
| JSON Pointer  | `json-ptr/src/lib.rs` — `JsonPointer`, `ROOT`, path navigation                  |
| JSON Patch    | `json-patch/src/lib.rs` — `Patch`, `PatchOperation`, `diff()`, `patch()`        |
| TS client     | `client/lib/patch-db.ts` — `PatchDB<T>` class                                   |
| TS patch lib  | `client/lib/json-patch-lib.ts` — client-side patch application                  |
| TS types      | `client/lib/types.ts` — `Revision`, `Dump`, `Update`, `PatchOp`                 |

Workspace members (`Cargo.toml`): `patch-db` (`core/`), `json-patch`, `json-ptr`, `patch-db-macro` (`macro/`), `patch-db-macro-internals` (`macro-internals/`), `patch-db-util` (`util/`).

## Build & test (run from the repo root)

The six Rust crates are members of the start-os root Cargo workspace:

```bash
cargo build -p patch-db                  # core crate (also -p json-patch / json-ptr / patch-db-macro / …)
cargo build -p patch-db --features debug # with tracing support
cargo test -p patch-db                   # core tests (uses proptest); also -p json-ptr / -p json-patch

cd shared-libs/crates/patch-db/client && npm install && npm run build   # TypeScript client → dist/
cd shared-libs/crates/patch-db/client && npm run check                  # type-check only
```

The web workspace consumes the built TS client; the monorepo's root `build/common.mk` builds `client/dist` as a prerequisite of the UIs.

## Operating rules

- **Wire format** — Rust and TS define `Revision`, `Dump`, and patch operations independently. Changes to one side must be mirrored in the other. See the cross-layer section in [CONTRIBUTING.md](CONTRIBUTING.md#making-changes).
- **Patch operations** — Only `add`, `remove`, and `replace` are used. The TS client does not implement `test`, `move`, or `copy`.
- **Immutable patch application** — The TS client applies patches by shallow-copying objects/arrays, not mutating in place. This is intentional for UI framework change detection.
- **`HasModel` derive** — Respects serde attributes (`rename_all`, `rename`, `flatten`, `tag`, `content`). Generated accessors follow the pattern `as_<field>()`, `as_<field>_mut()`, `into_<field>()`.
- **Error handling** — Rust uses `thiserror` with the `Error` enum in `core/src/lib.rs`. TS does not have formal error types.
