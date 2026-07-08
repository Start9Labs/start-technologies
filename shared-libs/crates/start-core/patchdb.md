# Patch-DB Patterns

## Model<T> and HasModel

Types stored in the database derive `HasModel`, which generates typed accessor methods on `Model<T>`:

```rust
#[derive(Debug, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct ServerInfo {
    pub version: Version,
    pub network: NetworkInfo,
    // ...
}
```

**Generated accessors** (one per field):

- `as_version()` — `&Model<Version>`
- `as_version_mut()` — `&mut Model<Version>`
- `into_version()` — `Model<Version>`

**`Model<T>` APIs:**

- `.de()` — Deserialize to `T`
- `.ser(&value)` — Serialize from `T`
- `.mutate(|v| ...)` — Deserialize, mutate, reserialize
- For maps: `.keys()`, `.as_idx(&key)`, `.insert()`, `.remove()`, `.contains_key()`

## Database Access

```rust
// Read-only snapshot
let snap = db.peek().await;
let version = snap.as_public().as_server_info().as_version().de()?;

// Atomic mutation
db.mutate(|db| {
    db.as_public_mut().as_server_info_mut().as_version_mut().ser(&new_version)?;
    Ok(())
}).await;
```

## TypedDbWatch<T>

Watch a JSON pointer path for changes and deserialize as a typed value. Requires `T: HasModel`.

### Construction

```rust
use patch_db::json_ptr::JsonPointer;

let ptr: JsonPointer = "/public/serverInfo".parse().unwrap();
let mut watch = db.watch(ptr).await.typed::<ServerInfo>();
```

### API

- `watch.peek()?.de()?` — Get current value as `T`
- `watch.changed().await?` — Wait until the watched path changes
- `watch.peek()?.as_field().de()?` — Access nested fields via `HasModel` accessors

### Usage Patterns

**Wait for a condition, then proceed:**

```rust
// Wait for DB version to match current OS version
let current = Current::default().semver();
let mut watch = db
    .watch("/public/serverInfo".parse().unwrap())
    .await
    .typed::<ServerInfo>();
loop {
    let server_info = watch.peek()?.de()?;
    if server_info.version == current {
        break;
    }
    watch.changed().await?;
}
```

**React to changes in a loop:**

```rust
// From net_controller.rs — react to host changes
let mut watch = db
    .watch("/public/serverInfo/network/host".parse().unwrap())
    .await
    .typed::<Host>();
loop {
    if let Err(e) = watch.changed().await {
        tracing::error!("DB watch disconnected: {e}");
        break;
    }
    let host = watch.peek()?.de()?;
    // ... process host ...
}
```

### Real Examples

- `net_controller.rs:469` — Watch `Hosts` for package network changes
- `net_controller.rs:493` — Watch `Host` for main UI network changes
- `service_actor.rs:37` — Watch `StatusInfo` for service state transitions
- `gateway.rs:1212` — Wait for DB migrations to complete before syncing
