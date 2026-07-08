# StartOS Version Bump Guide

This document explains how to bump the StartOS version across the entire codebase.

## Overview

When bumping from version `X.Y.Z-alpha.N` to `X.Y.Z-alpha.N+1`, you need to update files in multiple locations across the repository. The `// VERSION_BUMP` comment markers indicate where changes are needed.

## Files to Update

### 1. Core Rust Crate Version

**File: `core/Cargo.toml`**

Update the version string (line ~18):

```toml
version = "0.4.0-alpha.15" # VERSION_BUMP
```

**File: `core/Cargo.lock`**

This file is auto-generated. After updating `Cargo.toml`, run:

```bash
cd core
cargo check
```

This will update the version in `Cargo.lock` automatically.

### 2. Create New Version Migration Module

**File: `core/src/version/vX_Y_Z_alpha_N+1.rs`**

Create a new version file by copying the previous version and updating:

```rust
use exver::{PreReleaseSegment, VersionRange};

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_alpha_14};  // Update to previous version
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_alpha_15: exver::Version = exver::Version::new(
        [0, 4, 0],
        [PreReleaseSegment::String("alpha".into()), 15.into()]  // Update number
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_alpha_14::Version;  // Update to previous version
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_alpha_15.clone()  // Update version name
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    #[instrument(skip_all)]
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        // Add migration logic here if needed
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        // Add rollback logic here if needed
        Ok(())
    }
}
```

### 3. Update Version Module Registry

**File: `core/src/version/mod.rs`**

Make changes in **5 locations**:

#### Location 1: Module Declaration (~line 57)

Add the new module after the previous version:

```rust
mod v0_4_0_alpha_14;
mod v0_4_0_alpha_15;  // Add this
```

#### Location 2: Current Type Alias (~line 59)

Update the `Current` type and move the `// VERSION_BUMP` comment:

```rust
pub type Current = v0_4_0_alpha_15::Version; // VERSION_BUMP
```

#### Location 3: Version Enum (~line 175)

Remove `// VERSION_BUMP` from the previous version, add new variant, add comment:

```rust
    V0_4_0_alpha_14(Wrapper<v0_4_0_alpha_14::Version>),
    V0_4_0_alpha_15(Wrapper<v0_4_0_alpha_15::Version>), // VERSION_BUMP
    Other(exver::Version),
```

#### Location 4: as_version_t() Match (~line 233)

Remove `// VERSION_BUMP`, add new match arm, add comment:

```rust
            Self::V0_4_0_alpha_14(v) => DynVersion(Box::new(v.0)),
            Self::V0_4_0_alpha_15(v) => DynVersion(Box::new(v.0)), // VERSION_BUMP
            Self::Other(v) => {
```

#### Location 5: as_exver() Match (~line 284, inside #[cfg(test)])

Remove `// VERSION_BUMP`, add new match arm, add comment:

```rust
            Version::V0_4_0_alpha_14(Wrapper(x)) => x.semver(),
            Version::V0_4_0_alpha_15(Wrapper(x)) => x.semver(), // VERSION_BUMP
            Version::Other(x) => x.clone(),
```

### 4. SDK TypeScript Version (only on breaking SDK changes)

**File: `projects/start-sdk/lib/StartSdk.ts`**

**Only update this when the version bump includes breaking changes to the SDK.** The SDK `OSVersion` tracks compatibility for service developers, not the OS release cadence. Routine version bumps should skip this file.

Update the OSVersion constant (~line 64):

```typescript
export const OSVersion = testTypeVersion('0.4.0-alpha.15')
```

### 5. Web UI Package Version

**File: `web/package.json`**

Update the version field:

```json
{
  "name": "startos-ui",
  "version": "0.4.0-alpha.15",
  ...
}
```

**File: `web/package-lock.json`**

This file is auto-generated, but it's faster to update manually. Find all instances of "startos-ui" and update the version field.

## Verification Step

```
make
```

## VERSION_BUMP Comment Pattern

The `// VERSION_BUMP` comment serves as a marker for where to make changes next time:

- Always **remove** it from the old location
- **Add** the new version entry
- **Move** the comment to mark the new location

This pattern helps you quickly find all the places that need updating in the next version bump.

## Summary Checklist

- [ ] Update `core/Cargo.toml` version
- [ ] Create new `core/src/version/vX_Y_Z_alpha_N+1.rs` file
- [ ] Update `core/src/version/mod.rs` in 5 locations
- [ ] Run `cargo check` to update `core/Cargo.lock`
- [ ] Update `projects/start-sdk/lib/StartSdk.ts` OSVersion
  - ONLY do this if this OS version introduces new features or makes breaking changes that the SDK relies on. When in doubt, ask the user if breaking changes were made.
- [ ] Update `web/package.json` and `web/package-lock.json` version
- [ ] Verify all changes compile/build successfully

## Migration Logic

The `up()` and `down()` methods in the version file handle database migrations:

- **up()**: Migrates the database from the previous version to this version
- **down()**: Rolls back from this version to the previous version
- **pre_up()**: Runs before migration, useful for pre-migration checks or data gathering

If no migration is needed, return `Ok(Value::Null)` for `up()` and `Ok(())` for `down()`.

For complex migrations, you may need to:

1. Update `type PreUpRes` to pass data between `pre_up()` and `up()`
2. Implement database transformations in the `up()` method
3. Implement reverse transformations in `down()` for rollback support
