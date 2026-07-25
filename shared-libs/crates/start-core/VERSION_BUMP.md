# StartOS Version Bump Guide

How to bump the StartOS version across the codebase. The `// VERSION_BUMP` comment markers indicate where changes are needed.

## Where the version lives

StartOS versions carry an optional fourth **revision** segment — `0.4.0.1` — which SemVer, and so Cargo, cannot express. So the OS version does **not** live in a `Cargo.toml`:

| File                                         | Holds                             | Example        |
| -------------------------------------------- | --------------------------------- | -------------- |
| **root `package.json`**                      | **the OS version** (truth)        | `0.4.0.1`      |
| `shared-libs/crates/start-core/src/version/` | the migration graph `Current`     | `[0, 4, 0, 1]` |
| `projects/start-os/Cargo.toml`               | a label only                      | `0.4.0-rev.1`  |
| `shared-libs/crates/start-core/Cargo.toml`   | the crate version — the OS _line_ | `0.4.0`        |

- **Root `package.json` is the source of truth.** Everything that needs the OS version reads it: `build/env/version.sh` (used by `check-version.sh`, `basename.sh`, `debian/build.sh`), `derive_version` in `scripts/manage-release.sh`, `.github/workflows/startos-iso.yaml`, and `build.rs`, which bakes it in as `STARTOS_VERSION` for `bins::startos_version()`.
- **`projects/start-os/Cargo.toml` is a label**, spelling `X.Y.Z.N` as `X.Y.Z-rev.N` so the manifest acknowledges the release it belongs to. Nothing reads it for the OS version. Never treat it as a comparand: `0.4.0-rev.1` is a SemVer _prerelease_, so it sorts **below** `0.4.0` in both `semver` and `exver`. `manage-release.sh pre-check start-os` fails if the label doesn't match the version being cut.
- **`shared-libs/crates/start-core/Cargo.toml` tracks the OS line** (`0.4.0`, `0.4.1`, `0.5.0`), not the revision. Leave it alone for a revision bump — it feeds the generated man pages for all four products, several of which version independently.
- `version::tests::current_matches_manifest` asserts `Current::default().semver()` equals `package.json`, so the two cannot silently drift.

## Changelog

A version bump **pairs with a `CHANGELOG.md` entry** in the same change. Placement follows the repo-wide rule in the root [`AGENTS.md`](../../../AGENTS.md): freshly pull tags from origin first, keep the changelog's top heading at the prospective version (never `## [Unreleased]`), add your entry under it while that version is untagged, and cut a new heading only once it is a tagged release.

## Files to update

### 1. Root `package.json`

```json
{ "name": "startos-ui", "version": "0.4.0.1" }
```

`npm version` rejects a four-segment string — hand-edit it, then `npm install --package-lock-only` to update `package-lock.json`.

### 2. `projects/start-os/Cargo.toml`

Update the label to match (`0.4.0.1` → `0.4.0-rev.1`; a release with no revision is just itself). Run `cargo check` to update the root `Cargo.lock` — CI builds `--locked`, so commit it in the same change.

### 3. Create the version migration module

**`shared-libs/crates/start-core/src/version/vX_Y_Z_N.rs`** — copy the previous version and update. A revision release is plain digits with no prerelease segment:

```rust
use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0}; // previous version
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_1: exver::Version = exver::Version::new([0, 4, 0, 1], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0::Version; // previous version
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_1.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn up(self, _db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}
```

A pre-release version instead takes prerelease segments: `exver::Version::new([0, 4, 0], [PreReleaseSegment::String("alpha".into()), 15.into()])`.

**The node is mandatory even when the release migrates nothing.** Without it `pre_init` takes the `Ordering::Equal` branch, never calls `commit`, and `serverInfo.version` / `packageVersionCompat` never move — so the registry keeps re-offering the update forever.

`type Previous` must chain to the version immediately before it. Skipping one strands every server on the skipped version: the older image resolves the newer string to `Version::Other` and fails with `unknown version`, with no rollback.

### 4. Update the version module registry

**`shared-libs/crates/start-core/src/version/mod.rs`** — 5 locations, four already carrying the `// VERSION_BUMP` marker. Remove the marker from the previous version's line, add the new entry, and move the marker to it.

1. **Module declaration** — `mod v0_4_0_1;`
2. **`Current` type alias** — `pub type Current = v0_4_0_1::Version; // VERSION_BUMP`
3. **`Version` enum** — the new variant must go **before** `Other(exver::Version)`; the enum is `#[serde(untagged)]`, and `Other` is the catch-all.
   ```rust
   V0_4_0(Wrapper<v0_4_0::Version>),
   V0_4_0_1(Wrapper<v0_4_0_1::Version>), // VERSION_BUMP
   Other(exver::Version),
   ```
4. **`as_version_t()` match** — `Self::V0_4_0_1(v) => DynVersion(Box::new(v.0)), // VERSION_BUMP`
5. **`as_exver()` match** (inside `#[cfg(test)]`) — `Version::V0_4_0_1(Wrapper(x)) => x.semver(), // VERSION_BUMP`

### 5. Release-gated docs

`projects/start-os/docs/src/installing-startos.md` (and `update-040.md`) pin the GitHub release link to the shipping version. `manage-release.sh pre-check start-os` fails on a stale or `releases/latest` link.

### 6. SDK TypeScript version (only on breaking SDK changes)

**`projects/start-sdk/lib/StartSdk.ts`** — update `OSVersion` **only** when the bump includes breaking changes the SDK relies on. `OSVersion` tracks compatibility for service developers, not the OS release cadence; routine bumps skip it.

## Verification

```bash
cargo test -p start-core --features test version::   # incl. current_matches_manifest
./scripts/manage-release.sh pre-check start-os
```

## Summary checklist

- [ ] Update root `package.json` + `package-lock.json`
- [ ] Update the `projects/start-os/Cargo.toml` label; `cargo check` to refresh `Cargo.lock`
- [ ] Create `shared-libs/crates/start-core/src/version/vX_Y_Z_N.rs`
- [ ] Update `shared-libs/crates/start-core/src/version/mod.rs` in 5 locations
- [ ] Add the `CHANGELOG.md` entry under a new heading
- [ ] Bump the release link in `projects/start-os/docs/src/`
- [ ] Update `projects/start-sdk/lib/StartSdk.ts` `OSVersion` — **only** on breaking SDK changes
- [ ] `cargo test` + `pre-check` pass

## Migration logic

The `up()` and `down()` methods handle database migrations:

- **`pre_up()`** — runs before migration, for pre-migration checks or data gathering
- **`up()`** — migrates the database from the previous version to this one
- **`down()`** — rolls back

If no migration is needed, return `Ok(Value::Null)` from `up()` and `Ok(())` from `down()`. For complex migrations, set `type PreUpRes` to pass data from `pre_up()` into `up()`.
