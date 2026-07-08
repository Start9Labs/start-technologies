# Versions

StartOS uses Extended Versioning (ExVer) to manage package versions, allowing downstream maintainers to release updates without upstream changes.

## Version Format

```
[#flavor:]<upstream>[-upstream-prerelease]:<downstream>
```

| Component             | Description                          | Example       |
| --------------------- | ------------------------------------ | ------------- |
| `flavor`              | Optional variant for diverging forks | `#libre:`     |
| `upstream`            | Upstream project version (SemVer)    | `26.0.0`      |
| `upstream-prerelease` | Upstream prerelease suffix           | `-beta.1`     |
| `downstream`          | StartOS wrapper revision             | `0`, `1`, `2` |

> [!NOTE]
> ExVer allows a prerelease suffix on the downstream revision too (e.g. `:0-beta.0`), but Start9 packages don't use it — the downstream revision is always a plain integer. Prerelease suffixes appear only on the upstream side, when wrapping an upstream alpha/beta/rc.

### Flavor

Flavors are for diverging forks of a project that maintain separate version histories. Example: if a project forks into "libre" and "pro" editions that diverge significantly, each would have its own flavor prefix.

> [!NOTE]
> Do NOT use flavors for hardware variants (like GPU types) -- those should be handled via build configuration.

### Examples

| Version String  | Upstream        | Downstream |
| --------------- | --------------- | ---------- |
| `26.0.0:0`      | 26.0.0 (stable) | 0          |
| `26.0.0-rc.1:0` | 26.0.0-rc.1     | 0          |
| `0.13.5:0`      | 0.13.5 (stable) | 0          |
| `2.3.2:1`       | 2.3.2 (stable)  | 1          |

### Version Ordering

Versions are compared by:

1. Upstream version (most significant)
2. Upstream prerelease (stable > rc > beta > alpha)
3. Downstream revision

Example ordering (lowest to highest):

- `1.0.0-alpha.0:0`
- `1.0.0-beta.0:0`
- `1.0.0-rc.0:0`
- `1.0.0:0` (fully stable)
- `1.0.0:1`
- `1.1.0:0`

## Choosing a Version

When creating a new package:

1. **Select the latest stable upstream version** -- avoid prereleases (alpha, beta, rc) unless necessary.
2. **Match the Docker image tag** -- the version in `manifest/index.ts` `images.*.source.dockerTag` must match the upstream version.
3. **Match the git submodule** -- if using a submodule, check out the corresponding tag.
4. **Start downstream at 0** -- increment only when making wrapper-only changes.

### Version Consistency Checklist

Ensure these all match for upstream version `X.Y.Z`:

- The current version lives in `startos/versions/current.ts`
- Version string matches: `version: 'X.Y.Z:0'` in VersionInfo
- Docker tag matches: `dockerTag: 'image:X.Y.Z'` in `manifest/index.ts` (if using pre-built image)
- Git submodule checked out to `vX.Y.Z` tag (if applicable)

## File Structure

The latest version **always** lives in `startos/versions/current.ts`. The filename never changes as you bump — only its contents do. Historical versions that a migration needs to upgrade _from_ are kept as version-named files alongside it.

```
startos/versions/
├── index.ts          # VersionGraph: imports current, lists historical versions in `other`
├── current.ts        # The latest version (always this filename)
├── v1.0.0_0.ts       # Historical version 1.0.0:0, kept because a later migration upgrades from it
└── v1.1.0_0.ts       # Historical version 1.1.0:0, ditto
```

A brand-new package has only `index.ts` and `current.ts` — no historical files until a migration forces one out (see [When to Create a New Version File](#when-to-create-a-new-version-file)).

### current.ts Template

`current.ts` exports its `VersionInfo` under the stable name `current`. Keeping the export name fixed is what makes an in-place bump touch only this file — `index.ts` never changes.

```typescript
import { VersionInfo, IMPOSSIBLE } from '@start9labs/start-sdk'

export const current = VersionInfo.of({
  version: 'X.Y.Z:0',
  releaseNotes: {
    en_US: 'Initial release for StartOS',
    es_ES: 'Version inicial para StartOS',
    de_DE: 'Erstveeroffentlichung fuer StartOS',
    pl_PL: 'Pierwsze wydanie dla StartOS',
    fr_FR: 'Version initiale pour StartOS',
  },
  migrations: {
    up: async ({ effects }) => {},
    down: IMPOSSIBLE, // Use for initial versions or breaking changes
  },
})
```

### index.ts

```typescript
import { VersionGraph } from '@start9labs/start-sdk'
import { current } from './current'

export const versionGraph = VersionGraph.of({
  current,
  other: [], // Add historical versions here so migrations run when upgrading through them
})
```

### Historical Version File Naming

When a migration forces a version out of `current.ts` (see below), the spun-off file is named after the version it holds, in the same form as its [git tag](#git-tag-conventions): prefix with `v`, replace the `:` with `_`, and add `.ts`. The upstream portion keeps its dots; prerelease suffixes are left as-is.

| Version         | Filename            |
| --------------- | ------------------- |
| `26.0.0:0`      | `v26.0.0_0.ts`      |
| `26.0.0-rc.1:0` | `v26.0.0-rc.1_0.ts` |
| `2.3.2:1`       | `v2.3.2_1.ts`       |

A historical file's export is renamed to match the version, with every `.`, `:`, and `-` becoming `_` — e.g. `2.3.2:1` → `v_2_3_2_1`. Only `current.ts` uses the stable `current` export.

## Incrementing Versions

### When to Create a New Version File

The deciding question is **does this bump need a migration?**

**No migration (the common case): bump `current.ts` in place.** Edit `version` and `releaseNotes` in `startos/versions/current.ts` and you're done. Don't rename the file, don't touch the export name, don't touch `index.ts`, leave `other` as it is. Git history of `current.ts` preserves the prior release notes automatically, so there is no separate "keep the old notes" step.

**Migration needed: spin the old version off, then write a fresh `current.ts`.**

1. **Rename `current.ts` to the version it currently holds** — e.g. `v2.3.2_1.ts` (see [Historical Version File Naming](#historical-version-file-naming)), and rename its export from `current` to the matching `v_2_3_2_1`.
2. **Add that historical version to the `other` array** in `index.ts` so its migration runs when users upgrade through it.
3. **Create a new `startos/versions/current.ts`** exporting `current` with the new version string, release notes, and the `up`/`down` migration.

This keeps `versions/` lean: only versions that a migration upgrades _from_ survive as their own files; everything else is just the latest state of `current.ts`.

### Upstream Update

When the upstream project releases a new version:

1. Update git submodule to new tag
2. Update `dockerTag` in [manifest/index.ts](./manifest.md)
3. Update `current.ts` to the new upstream version (spin off a historical file only if the bump needs a migration — see above)
4. Reset downstream to 0

### Wrapper-Only Changes

When making changes to the StartOS wrapper without upstream changes:

1. Keep upstream version the same
2. Increment downstream revision
3. Apply the [migration rule](#when-to-create-a-new-version-file) — most wrapper-only bumps need no migration, so just edit `current.ts` in place

## Release Notes

`releaseNotes` renders as markdown in the StartOS UI. **Describe what changed in the release.** Read the upstream release notes / changelog for the bumped range, pull out the highlights that matter to a user (notable features, important fixes, security patches, breaking changes / required actions), and summarize them. Then link to the complete upstream release notes or changelog so the user can read the full detail. If the upstream release is genuinely trivial (e.g. a dependency-only patch), say that explicitly.

**Match the length to the content.** A small release is a sentence or two; a larger one earns bullets, and bold section headers (`**Features**`, `**Fixes**`, `**Internal**`) once it spans more than one category. Localize prose and headers in every locale; don't leave them in English.

```ts
// Small release: a sentence naming the bump + the key change, then the link.
releaseNotes: {
  en_US: `Updated Ghost to 6.38.0. Fixes a crash when restoring from backup and patches a moderate XSS vulnerability in the editor. Full notes: https://github.com/TryGhost/Ghost/releases/tag/v6.38.0`,
  // …other locales
},

// Larger release: highlights as bullets, then the link.
releaseNotes: {
  en_US: `Updated Ghost to 6.38.0.

- New: scheduled newsletter sends
- Fix: crash on backup restore
- Security: patched editor XSS

[Full release notes](https://github.com/TryGhost/Ghost/releases/tag/v6.38.0)`,
  // …
},

// Multiple categories spanning the wrapper and upstream: headers + bullets.
releaseNotes: {
  en_US: `Updated Ghost to 6.38.0 and bumped the start-sdk.

**Features**

- New: scheduled newsletter sends

**Fixes**

- Crash on backup restart

[Full upstream release notes](https://github.com/TryGhost/Ghost/releases/tag/v6.38.0)`,
  // …
},
```

Use a template literal (backticks) only when the note actually spans multiple lines, and never indent its content lines. If you genuinely can't find upstream notes, link to the upstream commit/tag comparison instead.

## Migrations

Migrations run when users update between versions:

```typescript
migrations: {
  up: async ({ effects }) => {
    // Code to migrate from previous version
    // Access volumes, update configs, etc.
  },
  down: async ({ effects }) => {
    // Code to rollback (if possible)
  },
}
```

Each migration also receives `progress` alongside `effects` — a [`FullProgressTracker`](./init.md) you can add a phase to and update for long-running migrations, so the work shows up in the update progress bar. Destructure it only when you use it: `up: async ({ effects, progress }) => …`.

Use `IMPOSSIBLE` for the `down` migration when:

- It is the initial version (nothing to roll back to)
- The migration involves breaking changes that cannot be reversed

```typescript
migrations: {
  up: async ({ effects }) => {
    // Migration logic
  },
  down: IMPOSSIBLE,
}
```

> [!WARNING]
> Migrations are only for migrating data that is _not_ migrated by the upstream service itself.

## setupOnInit

Use `sdk.setupOnInit()` to run setup logic during installation, restore, or container rebuild. It receives a `kind` parameter:

| Kind        | When it runs                        |
| ----------- | ----------------------------------- |
| `'install'` | Fresh install                       |
| `'restore'` | Restoring from backup               |
| `null`      | Container rebuild (no data changes) |

### Bootstrapping Config Files

Generate passwords, write initial config files, and seed stores on fresh install:

```typescript
// init/seedFiles.ts
export const seedFiles = sdk.setupOnInit(async (effects, kind) => {
  if (kind !== 'install') return

  const secretKey = utils.getDefaultString({ charset: 'a-z,A-Z,0-9', len: 32 })
  await storeJson.merge(effects, { secretKey })
  await configToml.merge(effects, {
    /* initial config */
  })
})
```

### Creating Tasks

Tasks reference actions, so they must be created in a `setupOnInit` that runs after actions are registered in the init sequence:

```typescript
// init/taskDisableRegistrations.ts
export const taskDisableRegistrations = sdk.setupOnInit(async (effects, kind) => {
  if (kind !== 'install') return
  await sdk.action.createOwnTask(effects, toggleRegistrations, 'important', {
    reason: 'After creating your admin account, disable registrations.',
  })
})
```

## Git Tag Conventions

Releases are published via git tags. The StartOS tag format is:

```
v{upstream_version}[-upstream-prerelease]_{wrapper_revision}
```

| Package version | Git tag          |
| --------------- | ---------------- |
| `26.0.0:0`      | `v26.0.0_0`      |
| `26.0.0-rc.1:0` | `v26.0.0-rc.1_0` |
| `0.13.5:2`      | `v0.13.5_2`      |

Conventions:

- **Underscore between upstream and wrapper.** The `:` from the version string becomes `_` in the tag — tags can't contain colons.
- **No package-name prefix.** The tag is just the version, not `myservice-v26.0.0_0`.
- **Keep the upstream prerelease suffix** (`-alpha.N` / `-beta.N` / `-rc.N`) when wrapping an upstream prerelease — it stays inline in the upstream portion. The downstream revision is always a plain integer with no suffix.
- **Push tags individually** (`git push origin <tag>`), not with `git push --tags`.
