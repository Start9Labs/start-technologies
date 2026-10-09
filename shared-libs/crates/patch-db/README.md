# patch-db

A database that tracks state updates as [RFC 6902 JSON Patches](https://tools.ietf.org/html/rfc6902). Enables observable, event-driven state management with a Rust backend and TypeScript client.

## Overview

patch-db stores your application state as a single JSON document. Instead of opaque writes, every mutation is recorded as a JSON Patch — a sequence of add/remove/replace operations. Subscribers receive only the patches relevant to the subtree they're watching, making it efficient for UIs that need to react to fine-grained state changes.

### Key properties

- **Event-sourced** — patches are the source of truth, not snapshots
- **Observable** — subscribers watch arbitrary subtrees via JSON Pointers and receive scoped patches in real time
- **Persistent** — the Rust backend writes to disk with CBOR serialization, automatic compaction, and crash-safe backup files
- **Type-safe** — derive macros on the Rust side; generic type parameters and deep `watch$()` overloads on the TypeScript side
- **Immutable values** — the Rust side uses `imbl_value::Value` for efficient structural sharing

## Quick start

### Rust

Add to your `Cargo.toml`:

```toml
[dependencies]
patch-db = { path = "../patch-db/core" }
```

```rust
use patch_db::PatchDb;
use json_ptr::ROOT;

#[tokio::main]
async fn main() -> Result<(), patch_db::Error> {
    let db = PatchDb::open("my.db").await?;

    // Write a value
    db.put(&ROOT, &serde_json::json!({ "count": 0 })).await?;

    // Read it back
    let dump = db.dump(&ROOT).await;
    println!("revision {}: {}", dump.id, dump.value);

    // Subscribe to changes
    let mut watch = db.watch(ROOT.to_owned()).await;
    // watch implements Stream — use it with tokio, futures, etc.

    Ok(())
}
```

### TypeScript

```typescript
import { PatchDB, Dump, Update } from 'patch-db'
import { Observable } from 'rxjs'

interface AppState {
  users: { [id: string]: { name: string; online: boolean } }
  settings: { theme: string }
}

// source$ delivers updates from the server (WebSocket, SSE, etc.)
const source$: Observable<Update<AppState>[]> = getUpdatesFromServer()

const db = new PatchDB<AppState>(source$)
db.start()

// Watch a deeply nested path — fully type-safe
db.watch$('settings', 'theme').subscribe(theme => {
  console.log('Theme changed:', theme)
})
```

## Rust reflection

The core crate's optional `reflect` feature enables `visit_rs::VisitFields`
for the `Dump` struct, providing raw declaration facts and static field traversal. RPC's `ts` + `patch-db` integration enables it and owns TypeScript
rendering and bridges. Dependency edges point from RPC to patch-db, not back to RPC.

## Further reading

- [ARCHITECTURE.md](ARCHITECTURE.md) — project structure, crate/package details, data flow, storage format
- [AGENTS.md](AGENTS.md) — environment setup, build commands, testing, code style

## License

MIT
