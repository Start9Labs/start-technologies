# Dependencies

Cross-service dependencies allow your service to interact with other StartOS services. Use them when your service needs to:

- Enforce configuration on a dependency (e.g., enable a feature)
- Register with a dependency (e.g., appservice registration)
- Read a dependency's interface URL at runtime

## Declaring Dependencies

Dependencies are declared in `manifest/index.ts`. Each dependency requires either `metadata` or `s9pk` to provide display info (title and icon). Both approaches achieve the same result -- they are two ways of providing the metadata:

```typescript
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
- `versionRange` — semver range the dependency must satisfy.
- `healthChecks` — names of the dependency's daemons (their `ready` IDs) or standalone health checks (`addHealthCheck` IDs) that should be passing.

If your service genuinely cannot operate before a dependency reaches a particular state (a file exists, an RPC responds, a config is generated), handle that at runtime in `setupMain` — poll the dependency, retry, or surface your own error. Don't rely on the dependency declaration to block startup for you.

## Creating Cross-Service Tasks

Use `sdk.action.createTask()` in `dependencies.ts` to trigger an action on a dependency. The action must be exported from the dependency's package.

```typescript
import { i18n } from './i18n'
import { sdk } from './sdk'
import { someAction } from 'dependency-package/startos/actions/someAction'

export const setDependencies = sdk.setupDependencies(async ({ effects }) => {
  await sdk.action.createTask(effects, 'dependency-id', someAction, 'critical', {
    input: {
      kind: 'partial',
      accept: [
        {
          /* one or more acceptable partial inputs */
        },
      ],
      set: {
        /* the value to pre-fill when none are accepted */
      },
    },
    when: { condition: 'input-not-matches', once: false },
    reason: i18n('Human-readable reason shown to user'),
  })

  return {
    'dependency-id': {
      kind: 'running',
      versionRange: '>=1.0.0:0',
      healthChecks: ['dependency-id'],
    },
  }
})
```

### API Signature

```typescript
sdk.action.createTask(
  effects,
  packageId: string,         // dependency service ID
  action: ActionDefinition,  // imported from the dependency package
  severity: 'critical' | 'important' | 'optional',
  options?: {
    input?: { kind: 'partial', accept: Partial<InputSpec>[], set: Partial<InputSpec> },
    when?: { condition: 'input-not-matches', once: boolean },
    reason: string,
    replayId?: string,       // prevents duplicate task execution
  }
)
```

> [!NOTE]
>
> - Import the action object from the dependency's published package.
> - The dependency must be listed in your `package.json` — see [Adding the Dependency to `package.json`](#adding-the-dependency-to-packagejson), including the required `overrides` entry.
> - `when: { condition: 'input-not-matches', once: false }` re-triggers until the action's input matches.
> - `replayId` prevents duplicate tasks across restarts. It defaults to `[package-id]:[action-id]`, so it changes whenever you rename the action, retarget the task, or point it at a different package — and the key you stop writing is left behind, still enforcing the old contract. Clearing it is your package's job: see [Retiring a replay key](./tasks.md#retiring-a-replay-key).

> [!IMPORTANT]
> **`accept` entries are matched against the dependency's _resolved_ action input, not its raw config file.** That input is the dependency action's prefill — its config parsed through its file model — so an optional field comes back carrying its **resolved default**, never a missing key. bitcoind's `prune`, for example, reads as the number `0` on an unpruned node (its file model coerces an absent `prune` to `0`), so `accept: [{ prune: 0 }]` matches an unpruned node exactly. Match the concrete value the input actually holds.
>
> An `accept` field value is compared for equality: `null` matches the literal value `null` — nothing else. It is **not** a wildcard and does **not** stand in for a defaulted or absent field. To leave a field unconstrained, **omit it** from the entry — an absent (`undefined`) key is not checked at all. (`undefined` means absence, not `null`: writing `undefined` for a field in a `set`/config would _delete_ that key.) To require a specific value, name it. Multiple entries mean "any of these matches."

## Reaching a Dependency at Runtime

A dependency is reached over the internal host bridge, by resolving its live bridge address (`10.0.3.1:<assigned port>`) with `sdk.host.get(...).const()`.

The mechanics — resolving a binding’s bridge address, the `.const()` restart matrix, `sdk.host.getBridgeAddress`, the Tor `fallbackPort` case, and where a backend-_selection_ value belongs — are all in **[Service-to-Service Networking](service-to-service.md)**. Read that page before dialing any dependency.

## Mounting Dependency Volumes

Mount a dependency's volume for direct file access in `main.ts`:

```typescript
const mounts = sdk.Mounts.of().mountVolume({ volumeId: 'main', subpath: null, mountpoint: '/data', readonly: false }).mountDependency({
  dependencyId: 'bitcoind',
  volumeId: 'main',
  subpath: null,
  mountpoint: '/mnt/bitcoind',
  readonly: true,
})
```

## Init Order

Dependencies are resolved during initialization in this order:

```
restoreInit -> versionGraph -> setInterfaces -> setDependencies -> actions -> setup
```

`setInterfaces` runs before `setDependencies`, so your service's interfaces are available when creating cross-service tasks.
