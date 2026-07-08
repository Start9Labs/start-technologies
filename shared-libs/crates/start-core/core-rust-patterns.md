# Utilities & Patterns

This document covers common utilities and patterns used throughout the StartOS codebase.

## Util Module (`core/src/util/`)

The `util` module contains reusable utilities. Key submodules:

| Module          | Purpose                                                      |
| --------------- | ------------------------------------------------------------ |
| `actor/`        | Actor pattern implementation for concurrent state management |
| `collections/`  | Custom collection types                                      |
| `crypto.rs`     | Cryptographic utilities (encryption, hashing)                |
| `future.rs`     | Future/async utilities                                       |
| `io.rs`         | File I/O helpers (create_file, canonicalize, etc.)           |
| `iter.rs`       | Iterator extensions                                          |
| `net.rs`        | Network utilities                                            |
| `rpc.rs`        | RPC helpers                                                  |
| `rpc_client.rs` | RPC client utilities                                         |
| `serde.rs`      | Serialization helpers (Base64, display/fromstr, etc.)        |
| `sync.rs`       | Synchronization primitives (SyncMutex, etc.)                 |

## Command Invocation (`Invoke` trait)

The `Invoke` trait provides a clean way to run external commands with error handling:

```rust
use crate::util::Invoke;

// Simple invocation
tokio::process::Command::new("ls")
    .arg("-la")
    .invoke(ErrorKind::Filesystem)
    .await?;

// With timeout
tokio::process::Command::new("slow-command")
    .timeout(Some(Duration::from_secs(30)))
    .invoke(ErrorKind::Timeout)
    .await?;

// With input
let mut input = Cursor::new(b"input data");
tokio::process::Command::new("cat")
    .input(Some(&mut input))
    .invoke(ErrorKind::Filesystem)
    .await?;

// Piped commands
tokio::process::Command::new("cat")
    .arg("file.txt")
    .pipe(&mut tokio::process::Command::new("grep").arg("pattern"))
    .invoke(ErrorKind::Filesystem)
    .await?;
```

## Guard Pattern

Guards ensure cleanup happens when they go out of scope.

### `GeneralGuard` / `GeneralBoxedGuard`

For arbitrary cleanup actions:

```rust
use crate::util::GeneralGuard;

let guard = GeneralGuard::new(|| {
    println!("Cleanup runs on drop");
});

// Do work...

// Explicit drop with action
guard.drop();

// Or skip the action
// guard.drop_without_action();
```

### `FileLock`

File-based locking with automatic unlock:

```rust
use crate::util::FileLock;

let lock = FileLock::new("/path/to/lockfile", true).await?;  // blocking=true
// Lock held until dropped or explicitly unlocked
lock.unlock().await?;
```

## Mount Guard Pattern (`core/src/disk/mount/guard.rs`)

RAII guards for filesystem mounts. Ensures filesystems are unmounted when guards are dropped.

### `MountGuard`

Basic mount guard:

```rust
use crate::disk::mount::guard::MountGuard;
use crate::disk::mount::filesystem::{MountType, ReadOnly};

let guard = MountGuard::mount(&filesystem, "/mnt/target", ReadOnly).await?;

// Use the mounted filesystem at guard.path()
do_something(guard.path()).await?;

// Explicit unmount (or auto-unmounts on drop)
guard.unmount(false).await?;  // false = don't delete mountpoint
```

### `TmpMountGuard`

Reference-counted temporary mount (mounts to `/media/startos/tmp/`):

```rust
use crate::disk::mount::guard::TmpMountGuard;
use crate::disk::mount::filesystem::ReadOnly;

// Multiple clones share the same mount
let guard1 = TmpMountGuard::mount(&filesystem, ReadOnly).await?;
let guard2 = guard1.clone();

// Mount stays alive while any guard exists
// Auto-unmounts when last guard is dropped
```

### `GenericMountGuard` trait

All mount guards implement this trait:

```rust
pub trait GenericMountGuard: std::fmt::Debug + Send + Sync + 'static {
    fn path(&self) -> &Path;
    fn unmount(self) -> impl Future<Output = Result<(), Error>> + Send;
}
```

### `SubPath`

Wraps a mount guard to point to a subdirectory:

```rust
use crate::disk::mount::guard::SubPath;

let mount = TmpMountGuard::mount(&filesystem, ReadOnly).await?;
let subdir = SubPath::new(mount, "data/subdir");

// subdir.path() returns the full path including subdirectory
```

## FileSystem Implementations (`core/src/disk/mount/filesystem/`)

Various filesystem types that can be mounted:

| Type           | Description                  |
| -------------- | ---------------------------- |
| `bind.rs`      | Bind mounts                  |
| `block_dev.rs` | Block device mounts          |
| `cifs.rs`      | CIFS/SMB network shares      |
| `ecryptfs.rs`  | Encrypted filesystem         |
| `efivarfs.rs`  | EFI variables                |
| `httpdirfs.rs` | HTTP directory as filesystem |
| `idmapped.rs`  | ID-mapped mounts             |
| `label.rs`     | Mount by label               |
| `loop_dev.rs`  | Loop device mounts           |
| `overlayfs.rs` | Overlay filesystem           |

## Other Useful Utilities

### `Apply` / `ApplyRef` traits

Fluent method chaining:

```rust
use crate::util::Apply;

let result = some_value
    .apply(|v| transform(v))
    .apply(|v| another_transform(v));
```

### `Container<T>`

Async-safe optional container:

```rust
use crate::util::Container;

let container = Container::new(None);
container.set(value).await;
let taken = container.take().await;
```

### `HashWriter<H, W>`

Write data while computing hash:

```rust
use crate::util::HashWriter;
use sha2::Sha256;

let writer = HashWriter::new(Sha256::new(), file);
// Write data...
let (hasher, file) = writer.finish();
let hash = hasher.finalize();
```

### `Never` type

Uninhabited type for impossible cases:

```rust
use crate::util::Never;

fn impossible() -> Never {
    // This function can never return
}

let never: Never = impossible();
never.absurd::<String>()  // Can convert to any type
```

### `MaybeOwned<'a, T>`

Either borrowed or owned data:

```rust
use crate::util::MaybeOwned;

fn accept_either(data: MaybeOwned<'_, String>) {
    // Use &*data to access the value
}

accept_either(MaybeOwned::from(&existing_string));
accept_either(MaybeOwned::from(owned_string));
```

### `new_guid()`

Generate a random GUID:

```rust
use crate::util::new_guid;

let guid = new_guid();  // Returns InternedString
```
