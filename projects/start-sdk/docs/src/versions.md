# Versions

StartOS uses Extended Versioning (ExVer) to manage package versions, allowing downstream maintainers to release updates without upstream changes.

## Version Format

```
[#flavor:]<upstream>[-upstream-prerelease]:<downstream>
```

| Component             | Description                          | Example       |
| --------------------- | ------------------------------------ | ------------- |
| `flavor`              | Optional variant for diverging forks | `#libre:`     |
| `upstream`            | Full upstream project version        | `26.0.0`      |
| `upstream-prerelease` | Upstream prerelease suffix           | `-beta.1`     |
| `downstream`          | StartOS wrapper revision             | `0`, `1`, `2` |

> [!NOTE]
> ExVer allows a prerelease suffix on the downstream revision too (e.g. `:0-beta.0`), but Start9 packages don't use it — the downstream revision is always a plain integer. Prerelease suffixes appear only on the upstream side, when wrapping an upstream alpha/beta/rc.

### Preserve the upstream version

ExVer does not require SemVer's three numeric components. Its numeric portion supports any number of dot-separated components, with an optional prerelease suffix. Preserve every upstream component, including build or patch counters beyond the third position. Do not truncate, pad to three components, or reinterpret an upstream version as SemVer.

For example, Collabora's `26.04.4.2.1` becomes `26.4.4.2.1:0`, not `26.4.4:0`. ExVer normalizes numeric leading zeros; that normalization does not discard a component. Keep the exact upstream spelling where an artifact requires it, such as a Docker tag.

Some projects use CalVer, a single build number, or another release scheme rather than SemVer. If an upstream identifier cannot be represented directly in ExVer, document an order-preserving mapping in `UPDATING.md`, retaining the original identifier in the artifact pin and release notes. Distinct upstream releases must remain distinct package versions. Review scope follows [upstream's versioning scheme and release impact](maintaining-a-package.md#scale-scrutiny-to-the-size-of-the-jump), not the number of components.

### Flavor

Flavors are for diverging forks of a project that maintain separate version histories. Example: if a project forks into "libre" and "pro" editions that diverge significantly, each would have its own flavor prefix.

> [!NOTE]
> Do NOT use flavors for hardware variants (like GPU types) -- those should be handled via build configuration.

### Examples

| Version String  | Upstream            | Downstream |
| --------------- | ------------------- | ---------- |
| `26.0.0:0`      | 26.0.0 (stable)     | 0          |
| `26.0.0-rc.1:0` | 26.0.0-rc.1         | 0          |
| `0.13.5:0`      | 0.13.5 (stable)     | 0          |
| `2.3.2:1`       | 2.3.2 (stable)      | 1          |
| `26.4.4.2.1:0`  | 26.4.4.2.1 (stable) | 0          |
| `2026.9:0`      | 2026.9 (stable)     | 0          |
| `9982:0`        | 9982 (stable)       | 0          |

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

Ensure these all identify the same complete upstream release:

- The current version lives in `startos/versions/current.ts`
- `VersionInfo.version` preserves the full upstream version, followed by `:<revision>`
- `images.*.source.dockerTag` in `manifest/index.ts` resolves to that release (if using a pre-built image); retain artifact-specific padding, prefixes, and suffixes here
- The git submodule is checked out to that release's exact tag or commit (if applicable)

## File Structure

The latest version **always** lives in `startos/versions/current.ts`. The filename never changes as you bump — only its contents do. Historical versions needed to preserve migration steps and connections are kept as version-named files alongside it.

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

Write `version` as a quoted string literal — never a template literal or an expression built from a constant such as an upstream-version pin. The release workflow reads it from the file's text to name the git tag, and fails on anything else.

Chain `.satisfies(version)` to declare full compatibility with another version of this package, or `.satisfies(packageId, version)` to provide another package's contract. See [Compatible Providers](dependencies.md#compatible-providers) for the declaration format, compatibility obligations, and provider selection.

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

**A migration belongs to the version that introduced it, permanently.** It is never carried forward into a successor, and never justified by being idempotent — correctness here must not rest on a migration body being safe to apply twice.

That principle decides the file layout, so the deciding question is **does the version currently in `current.ts` carry a migration?** — not whether the bump you are making needs one. That only decides what you write into the new `current.ts`.

**The outgoing version's migration is empty (the common case): bump `current.ts` in place.** Empty means neither `up` nor `down` performs a data conversion, and no `migrations.other` entry is pinned to this version (below). The template's empty `up` with `down: IMPOSSIBLE` meets that rule, and the block stays as it is on the bump. Edit `version` and `releaseNotes` in `startos/versions/current.ts`, adding the new version's own migration if it needs one. Don't rename the file, don't touch the export name, don't touch `index.ts`, leave `other` as it is. Git history of `current.ts` preserves the prior release notes automatically, so there is no separate "keep the old notes" step.

**The outgoing version carries a migration or a pinned connection: spin it off, then write a fresh `current.ts`.** A `migrations.other` entry is **pinned** when it is valid only for this version's data format, like a bridge to another flavor at the version where their formats match. Moving it onto a newer version would claim the newer format crosses unchanged, so it earns its version a historical file even when its functions do no work. An entry that **follows `current`** stays in `current.ts` and moves with every bump: one that brings another flavor's or line's data into whatever version is newest, such as a flavor's arrival edge from each of another flavor's series.

1. **Rename `current.ts` to the version it currently holds** — e.g. `v2.3.2_1.ts` (see [Historical Version File Naming](#historical-version-file-naming)), and rename its export from `current` to the matching `v_2_3_2_1`.
2. **Add that historical version to the `other` array** in `index.ts` so paths can still traverse its migrations and pinned connections.
3. **Create a new `startos/versions/current.ts`** exporting `current` with the new version string, release notes, and its own migration if it needs one — never the one you just spun off.

> [!WARNING]
> Bumping in place over a version that carries a migration **deletes that migration from the graph**. Users upgrading through it can silently skip the conversion. Dropping a pinned connection can break a flavor-switch path; moving it to `current` changes where the path crosses flavors. Preserve the version that owns it, even when the migration body is safe to re-run.

This keeps `versions/` focused on the steps and connections needed by migration paths, rather than on release history.

### Why Released Versions Don't Need to Be Declared

A released version is **not** a reason to add it to `other`. Keep definitions needed by migration paths. This trips people up, so here is the mechanism.

`VersionGraph` does not only add a vertex per version you declare. For every version whose `up` is not `IMPOSSIBLE`, it also synthesizes a **range vertex** — an edge _into_ that version, covering the whole gap beneath it (`<X` if it is the lowest declared version, otherwise `>=prev && <X`). A package with `other: []` and `current` at `1.0.0:3` therefore has a graph of exactly two vertices:

```
<1.0.0:3  --[up]-->  1.0.0:3
```

Any installed version of the same flavor below `current` falls inside that range and migrates to `current` **in a single hop**, running `current`'s `up` migration once. This holds whether or not that version was ever declared, and whether or not it was ever released — including versions you shipped as sideloadable `.s9pk`s outside a registry.

Keep a declared historical node when a migration path needs its data conversion, pinned connection, or range boundary. A historical flavor bridge is one such node ([example below](#composing-down-across-flavor-and-up)). Release history alone belongs in git history of `current.ts`.

> [!NOTE]
> With an allowed `up`, a higher `current` accepts earlier versions of the same flavor through its range vertex. Those releases need no nodes of their own unless a migration path needs their steps or connections.

### canMigrateFrom Is Derived, Not Curated

The manifest's `canMigrateFrom` / `canMigrateTo` fields are **computed from the graph** — a reverse/forward search from `current`, which `setupManifest` serializes into the manifest at pack time. They include versions reachable through multiple edges, including explicit cross-flavor connections. With `other: []`, an allowed `up` already accepts every lower version of the same flavor through the range vertex above. Add historical nodes to preserve the steps and connections a path needs, rather than to enumerate releases.

### Upstream Update

When the upstream project releases a new version:

1. Update git submodule to new tag
2. Update `dockerTag` in [manifest/index.ts](./manifest.md)
3. Update `current.ts` to the new upstream version (spin off a historical file first if the version it currently holds carries a data conversion or pinned connection — see above)
4. Reset downstream to 0

### Wrapper-Only Changes

When making changes to the StartOS wrapper without upstream changes:

1. Keep upstream version the same
2. Increment downstream revision
3. Apply the [migration rule](#when-to-create-a-new-version-file) — preserve any data conversions or pinned connections on the version being replaced

### A version is released once the reference registry serves it

`tagAndRelease` re-releases a version in place on every push until the registry named by `REFERENCE_REGISTRY` serves it ([Project Structure — .github/workflows/](./project-structure.md#githubworkflows)), so only that registry makes a version permanent. A follow-up to a version it does not serve yet ships under that same version, its notes added to the ones already there. Check before you bump: `start-cli -r <reference-registry> registry package get <id> short --format json`. Release notes likewise cover every change since the version the reference registry last served; the notes of a version that never reached it carry forward into the next.

**Before you claim a version, check the package's open pull requests for one that already claims it.** Two branches on one version ship different bytes under the same string. Whether to fold one into the other or move one to the next revision is for the people who own both to decide, not for either branch to settle alone.

## Release Notes

`releaseNotes` renders as markdown in the StartOS UI. **Describe what changed in the release.** Read the upstream release notes / changelog for the bumped range, pull out the highlights that matter to a user (notable features, important fixes, security patches, breaking changes / required actions), and summarize them. Then link to the complete upstream release notes or changelog so the user can read the full detail. If the upstream release is genuinely trivial (e.g. a dependency-only patch), say that explicitly.

**Say what a user observes, never how it was done.** Name the difference someone running the service will notice: a feature, a fix described by its symptom, a required action. Leave out the mechanism (the module, flag or config key that changed), the package's internals (an SDK bump, a refactor, a dependency refresh) and the history of a defect ("introduced in 2.37.0", "previously…"); they belong in the commit message and the PR body. A wrapper change nobody can observe gets no line of its own. Never state the minimum StartOS version: only a server that already meets it is shown the notes, and StartOS offers the update to no other. Every instruction must be one a StartOS user can carry out from the StartOS UI or the service itself — never a host command (`sysctl`, `apt`) and never a pointer to guidance the note doesn't link.

**Release notes are the only home for what an update asks of the user.** A step to take before or after updating, a warning, a credential to rotate, a client to upgrade: each goes here and nowhere else. `instructions.md` and `README.md` describe the service as it is to someone who never ran an older version, so an update adds nothing to them unless it changes that description ([Writing Instructions — What does not belong](./writing-instructions.md#what-does-not-belong-in-instructions)).

**Match the length to the content.** A small release is a sentence or two; a larger one earns bullets, and bold section headers (`**Features**`, `**Fixes**`, `**Security**`) once it spans more than one category. Localize prose and headers in every locale; don't leave them in English.

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
  en_US: `Updated Ghost to 6.38.0 and added a Reset Password action.

**Features**

- New: scheduled newsletter sends
- New: Reset Password action

**Fixes**

- Crash on backup restore

[Full upstream release notes](https://github.com/TryGhost/Ghost/releases/tag/v6.38.0)`,
  // …
},
```

Use a template literal (backticks) only when the note actually spans multiple lines, and never indent its content lines. If you genuinely can't find upstream notes, link to the upstream commit/tag comparison instead.

## Migrations

A version's `migrations` are the edges StartOS walks to carry a package's data from one version to another:

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

`down: IMPOSSIBLE` removes that version's ordinary downward edges. Explicit `migrations.other` edges remain independent. StartOS refuses a downgrade when the outgoing and incoming graphs cannot compose a path to it ([below](#how-startos-runs-a-migration-path)).

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

### How Edges Compose Within a Graph

`VersionGraph.of({ current, other })` supplies the version definitions used to construct one package's graph. The SDK sorts the declared versions separately for each flavor. A version's ordinary `up` points from the preceding declared version to it; its ordinary `down` points back. The SDK also creates range vertices covering the gaps between declared versions ([above](#why-released-versions-dont-need-to-be-declared)). These ordinary edges connect versions of the same flavor.

`migrations.other` adds explicit directed connections to that graph. Each key supplies a range vertex, with edges in the directions declared, and connects matching declared version vertices too. A path can compose ordinary and explicit edges through historical nodes: down within one flavor, across an explicit connection, then up within another flavor. The explicit connection need not touch either package's `current`.

For ordinary `up` and `down`, an omitted function leaves a traversable edge with no data conversion; `IMPOSSIBLE` removes that direction. Within a `migrations.other` entry, each direction exists when its function is supplied. An empty function there still declares a connection.

Traversal follows directed edges along a shortest path by edge count. It can skip an intermediate version when an explicit shortcut provides a shorter path. Each successful edge stores its destination as the data version; a range destination can be the handoff between packages. A migration aimed at an exact version settles the stored data version to that exact version when it reaches a matching range.

### How StartOS Runs a Migration Path

StartOS records a **data version** for every installed package: the version the data on disk was last migrated to. Each migration step advances it. An update, a downgrade, and a switch to another flavor of the same package id are all one path from that data version to the incoming package's `current`, and **the path can run through two version graphs: the outgoing package's, then the incoming package's.**

StartOS splits the path by choosing a `target` data version for the outgoing package's uninit, from the outgoing manifest's `canMigrateTo` and the incoming manifest's `canMigrateFrom`:

| When                                                           | `target`                      | Outgoing package's graph runs | Incoming package's graph runs    |
| -------------------------------------------------------------- | ----------------------------- | ----------------------------- | -------------------------------- |
| The incoming package accepts the current data version          | the current data version      | nothing                       | the whole path                   |
| Otherwise, the outgoing package can reach the incoming version | the incoming version          | the whole path                | nothing                          |
| Otherwise                                                      | the overlap of the two ranges | the path into the overlap     | the path from there to `current` |

When the two ranges don't overlap, StartOS refuses the update. The outgoing package's `versionGraph` uninit walks its shortest path from the data version to `target`. The incoming package's `versionGraph` init then walks its own shortest path from wherever the data version is left to its `current`. StartOS chooses the handoff in the table's order; each package searches its own graph for its half of the path.

So which package runs a migration depends on the path, not on the version that declares it:

- **An update runs entirely in the incoming package when it accepts the current data version**, the first row. Same-flavor updates with an unbroken chain of allowed `up` edges normally take this route.
- **A version's `down` runs in that version's own code** whenever the outgoing half steps down from it: a rollback, or a switch to a flavor that accepts only older data.
- **An edge to another flavor or release line can be declared by either package** ([below](#edges-to-other-flavors-and-release-lines)), and runs in whichever half of the path it falls in.
- **Work that must happen when the data leaves a version belongs in that version's migrations**, not in uninit. Uninit's `target` is a data version, not the incoming version: on a compatible update it is the version already installed, the same value a shutdown passes. Only an uninstall is identifiable, by `target: null` ([Uninit](./init.md#uninit)).

### Edges to Other Flavors and Release Lines

`migrations.other` declares explicit edges between a version and a version range, such as another flavor with the same package id or another release line. Its `up` migrates data from a version in that range to the declaring version; its `down` migrates from the declaring version to that range. **The names describe direction relative to the declaring version, not numerical ordering across flavors.** Supply each direction the package supports.

```typescript
migrations: {
  up: async ({ effects }) => {},
  down: IMPOSSIBLE,
  other: {
    // Bitcoin Core 29.x data, arriving at or leaving this Knots version
    '^29': {
      up: async ({ effects }) => { /* Core → this version */ },
      down: async ({ effects }) => { /* this version → Core */ },
    },
  },
},
```

These edges compose with the rest of the graph. A declaration in either package is enough for a direction when the surrounding edges complete the path. `VersionGraph.of({ other })` supplies historical version definitions; `migrations.other` supplies explicit connections on one of those definitions.

### Composing Down, Across Flavor, and Up

Suppose two flavors of the same package id have the following version definitions. The function names stand for the actual data conversions:

| Definition | Version      | Ordinary `up`   | Ordinary `down` |
| ---------- | ------------ | --------------- | --------------- |
| `a2`       | `#a:2.0.0:0` | `IMPOSSIBLE`    | `IMPOSSIBLE`    |
| `a3`       | `#a:3.0.0:0` | `migrateA2ToA3` | `migrateA3ToA2` |
| `b2`       | `#b:2.0.0:0` | `IMPOSSIBLE`    | `IMPOSSIBLE`    |
| `b3`       | `#b:3.0.0:0` | `migrateB2ToB3` | `migrateB3ToB2` |

The data formats can cross flavors at version 2. Declare that bridge on `a2` by adding this field to its `migrations`:

```typescript
other: {
  '#b:2.0.0:0': {
    up: migrateB2ToA2,
    down: migrateA2ToB2,
  },
},
```

Each package includes its own historical bridge endpoint:

```typescript
const graphA = VersionGraph.of({ current: a3, other: [a2] })
const graphB = VersionGraph.of({ current: b3, other: [b2] })
```

**Switching A3 → B3** composes three edges, even though there is no direct edge between the current versions:

```text
#a:3.0.0:0 --[a3.down]--> #a:2.0.0:0
           --[a2.other['#b:2.0.0:0'].down]--> #b:2.0.0:0
           --[b3.up]--> #b:3.0.0:0
```

The incoming B graph accepts B2 data but not A3 data. The outgoing A graph reaches B2 but not B3. Their ranges overlap at B2, so StartOS chooses B2 as the handoff:

1. **Outgoing A uninit** runs `a3.down`, then `a2`'s explicit `down` to B2.
2. **Incoming B init** runs `b3.up` from B2 to B3.

**Switching B3 → A3** uses the reverse directions of the same declarations:

```text
#b:3.0.0:0 --[b3.down]--> #b:2.0.0:0
           --[a2.other['#b:2.0.0:0'].up]--> #a:2.0.0:0
           --[a3.up]--> #a:3.0.0:0
```

The handoff is again B2. Outgoing B uninit runs `b3.down`; incoming A init runs `a2`'s explicit `up`, then `a3.up`. The same bridge belongs to outgoing code in the first switch and incoming code in the reverse switch. The two graphs retain their own definitions and execute in sequence.

This example permits both switches because all six directed conversions exist. Supplying just the bridge's `up` supports B2 → A2; the A2 → B2 direction requires its `down`. Likewise, a blocked ordinary downward edge can prevent reaching the bridge in the first place. Retain `a2` when A advances beyond version 3: its bridge is pinned to version 2 and remains part of the composed path ([version-file rule](#when-to-create-a-new-version-file)).

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
