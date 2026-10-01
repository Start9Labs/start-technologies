# Dependencies

Define a service's dependencies once in `startos/dependencies.ts`. Each base requirement is packed into the service's manifest and published in its registry metadata. StartOS records required base requirements as current dependencies during initialization; runtime requirements refine them and activate optional integrations.

## Declare the Base Requirement

```typescript
<<<<<<< HEAD
=======
dependencies: {
  // Provide metadata directly
  synapse: {
    description: 'Needed for Matrix homeserver',
    optional: false,
    metadata: {
      title: 'Synapse',
      icon: '../synapse-wrapper/icon.png',
    },
  },

  // Extract metadata from an s9pk file
  electrs: {
    description: 'Provides an index for address lookups',
    optional: true,
    s9pk: 'https://github.com/org/repo/releases/download/v1.0/electrs.s9pk',
  },

  // s9pk: null when no s9pk URL is available
  'other-service': {
    description: 'Optional integration',
    optional: true,
    s9pk: null,
  },
}
```

### Naming a Dependency

**If a dependency has multiple flavors, refer to it by a generic term.** The most prominent example today is `bitcoind`: dependents should call it **Bitcoin**, never Bitcoin Core or Bitcoin Knots. Use the `metadata.title` you declared for it above — that's the name the UI shows.

This covers user-facing text only: release notes, `instructions.md`, action and config-field descriptions, task reasons, i18n dictionaries. Import paths and mount points are code, not messaging.

### Adding the Dependency to `package.json`

Importing a dependency's types — its manifest, its interface constants, an action object — means installing its packaging repo as an npm dependency, pinned to a branch:

```json
"dependencies": {
  "@start9labs/start-sdk": "2.0.9",
  "synapse-startos": "github:Start9Labs/synapse-startos#next"
},
"overrides": {
  "@start9labs/start-sdk": "$@start9labs/start-sdk"
}
```

Nothing from that repo ships in your package. StartOS installs each service separately; the dependency is present purely so your code compiles against its real types instead of hand-copied string literals.

> [!IMPORTANT]
> **The `overrides` entry is required whenever you depend on another packaging repo.** That repo declares its own `@start9labs/start-sdk` version, and if it differs from yours by even a patch, npm nests a second copy of the SDK under it. Both copies then get bundled into your `javascript/index.js` — roughly doubling it — and the two sets of SDK classes are distinct types at runtime.
>
> The `$` form points at your own root pin, so it keeps tracking your SDK version with no further edits. It also makes a genuine mismatch _loud_: your `tsconfig.json` includes `node_modules/**/startos`, so the dependency's source is type-checked against **your** SDK. If it uses an API your version doesn't have, you get a build error naming the file — rather than a silently duplicated SDK. Fix that by bumping your own SDK to match.
>
> It costs nothing where it isn't needed: with no nested copy to collapse, the lockfile and the built bundle are byte-identical.

## What `setupDependencies` Returns

The object you return from `setupDependencies()` declares what state each dependency should be in for your service to be considered "fully operational." It drives the **warning UI** the user sees on the service detail page — if a listed dependency isn't installed, isn't running, or has a listed health check failing, StartOS shows them a warning indicator and links them to the offending service.

It does **not** gate your service's startup. Your service starts whenever the user starts it, regardless of dependency state. The fields:

- `kind: 'running'` — user should have this dependency running. `kind: 'exists'` — user only needs it installed.
- `versionRange` — ExVer range the dependency must satisfy; see [Versions](versions.md).
- `healthChecks` — names of the dependency's daemons (their `ready` IDs) or standalone health checks (`addHealthCheck` IDs) that should be passing.

If your service genuinely cannot operate before a dependency reaches a particular state (a file exists, an RPC responds, a config is generated), handle that at runtime in `setupMain` — poll the dependency, retry, or surface your own error. Don't rely on the dependency declaration to block startup for you.

## Creating Cross-Service Tasks

Use `sdk.action.createTask()` in `dependencies.ts` to trigger an action on a dependency. The action must be exported from the dependency's package.

```typescript
import { i18n } from './i18n'
>>>>>>> bed7e9467 (docs(start-sdk): preserve full upstream versions without assuming SemVer (#4129))
import { sdk } from './sdk'
import { i18n } from './i18n'

const bitcoin = sdk.Dependency.required('bitcoind', {
  description: i18n('Needed for blockchain data'),
  metadata: { title: 'Bitcoin', icon: 'https://example.com/bitcoin-icon.png' },
  versionRange: '>=28.4:17',
  kind: 'running',
  healthChecks: ['bitcoind'],
})

export const dependencies = sdk.Dependencies.of().addDependency(bitcoin)
```

Pass `dependencies` to `buildManifest(versionGraph, sdkManifest, dependencies)` in `startos/index.ts` and to `sdk.setupInit(..., dependencies, ...)` in `startos/init/index.ts`.

The base is serializable at pack time, without effects: every dependency needs a description (which may be `null`), inline title and icon, version range, kind, and health checks when `kind` is `running`. Write `'*'` when any version is accepted. Name multi-flavor dependencies generically: for `bitcoind`, use **Bitcoin**, not Bitcoin Core or Bitcoin Knots. The base is the loosest requirement the service accepts in any configuration.

A range is compared against the installed version and any versions declared in the dependency's `satisfies` list. One declared version must satisfy a complete `&&` conjunction; `||` branches may match different declared versions. Negated ranges and `!=` veto a matching branch.

## Optional and Conditional Dependencies

An optional dependency needs an `enabled` function. It appears among current dependencies when enabled and the runtime requirements are published. Required dependencies appear from the manifest even when the init script does not publish runtime requirements.

```typescript
const lightning = sdk.Dependency.optional('lnd', {
  description: i18n('Lightning backend'),
  metadata: { title: 'LND', icon: 'https://example.com/lnd-icon.png' },
  versionRange: '>=0.20:0',
  kind: 'exists',
  enabled: async ({ effects }) => (await config.read(c => c.backend).const(effects)) === 'lnd',
}).withDynamicNarrowing(async ({ effects }) => ((await config.read(c => c.features).const(effects)).advanced ? { versionRange: '>=0.21:0', kind: 'running', healthChecks: ['lnd'] } : null))
```

A dynamic range is intersected with the base. A disjoint range throws; a broader range cannot loosen the published requirement. StartOS applies the same base when an enabled optional dependency is reported through `effects.setDependencies`, including its running status and health checks. `kind` may tighten from `exists` to `running`, and health checks may only be added. Returning `null` keeps the base. `.const(effects)` makes `enabled`, narrowing, and `withInit` reactive, and each has its own effects context. A watched change in `enabled` reruns only `enabled`: the runtime requirements are republished when its result changes, and enabling the dependency reruns its narrowing and init handlers. A watched change in the narrowing republishes the requirements when the result changes. A watched change in a `.withInit` handler reruns only that handler.

Both required and optional dependencies can use `.withInit` to create cross-service tasks. Chain multiple `.withInit` calls to add handlers in order. Handlers run only while the dependency is enabled, after the combined runtime requirements are published. While an optional dependency is disabled, StartOS keeps the tasks the service created on it but hides them, and a critical one does not prevent the service from starting. They return when the dependency is enabled again.

```typescript
const bitcoin = sdk.Dependency.required('bitcoind', {
  description: i18n('Blockchain data'),
  metadata: { title: 'Bitcoin', icon: 'https://example.com/bitcoin-icon.png' },
  versionRange: '>=28.4:17',
  kind: 'running',
  healthChecks: ['bitcoind'],
}).withInit(async effects => {
  await sdk.action.createTask(effects, 'bitcoind', someAction, 'critical', {
    input: {
      kind: 'partial',
      accept: [
        {
          /* matching input */
        },
      ],
      set: {
        /* prefill */
      },
    },
    when: { condition: 'input-not-matches', once: false },
    reason: i18n('Configure Bitcoin for this service'),
  })
})
```

See [Tasks](tasks.md) for action input matching and replay IDs. Importing another package's action or types requires adding its repo to `package.json` and using `"overrides": { "@start9labs/start-sdk": "$@start9labs/start-sdk" }` to avoid a second SDK copy.

StartOS reports unsatisfied runtime dependencies, but their declaration does not prevent the service from starting. If the service cannot operate until a dependency is ready, handle that in `setupMain` and surface its own status or retry. To check the runtime requirements in an action or main, call `dependencies.check(effects)`.

## Reaching a Dependency

Resolve its live address with `sdk.host.getBridgeAddress`, as described in [Service-to-Service Networking](service-to-service.md). For a volume mount, use `.mountDependency({ dependencyId, volumeId, subpath, mountpoint, readonly: true })` in the `Mounts` chain. A missing dependency volume makes the mount fail.

The normal init order is `restoreInit -> versionGraph -> setInterfaces -> actions -> dependencies -> setup`. Put `actions` before `dependencies` when `withInit` creates tasks for registered actions.
