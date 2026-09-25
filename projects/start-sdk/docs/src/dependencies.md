# Dependencies

Define a service's dependencies once in `startos/dependencies.ts`. Each base requirement is packed into the service's manifest and published in its registry metadata. StartOS records required base requirements as current dependencies during initialization; runtime requirements refine them and activate optional integrations.

## Declare the Base Requirement

```typescript
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

A dynamic range is intersected with the base. A disjoint range throws; a broader range cannot loosen the published requirement. StartOS applies the same base when an enabled optional dependency is reported through `effects.setDependencies`, including its running status and health checks. `kind` may tighten from `exists` to `running`, and health checks may only be added. Returning `null` keeps the base. `.const(effects)` makes `enabled`, narrowing, and `withInit` reactive. Requirement changes republish the combined list. Each `.withInit` handler has its own effects context, so a watched change reruns only that handler, even when the same dependency has multiple init handlers.

Both required and optional dependencies can use `.withInit` to create cross-service tasks. Chain multiple `.withInit` calls to add handlers in order. For optional dependencies, pass `.withInit(handler, ['dependency-id:action-id'])` for **each handler**, listing the task replay IDs that handler creates. When the integration is disabled, each handler clears its own IDs, including after a container restart; enabling it reruns each handler. Initially all handlers run in order before the combined runtime requirements are published. A task created with `sdk.action.createTask` defaults to the replay ID `[package-id]:[action-id]`; specify a different ID in both the task creation and the list if needed.

```typescript
const bitcoin = sdk.Dependency.required('bitcoind', {
  description: i18n('Blockchain data'),
  metadata: { title: 'Bitcoin', icon: 'https://example.com/bitcoin-icon.png' },
  versionRange: '>=28.4:17',
  kind: 'running',
  healthChecks: ['bitcoind'],
}).withInit(
  async effects => {
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
  },
  ['bitcoind:some-action'],
)
```

See [Tasks](tasks.md) for action input matching and replay IDs. Importing another package's action or types requires adding its repo to `package.json` and using `"overrides": { "@start9labs/start-sdk": "$@start9labs/start-sdk" }` to avoid a second SDK copy.

StartOS reports unsatisfied runtime dependencies, but their declaration does not prevent the service from starting. If the service cannot operate until a dependency is ready, handle that in `setupMain` and surface its own status or retry. To check the runtime requirements in an action or main, call `dependencies.check(effects)`.

## Reaching a Dependency

Resolve its live address with `sdk.host.getBridgeAddress`, as described in [Service-to-Service Networking](service-to-service.md). For a volume mount, use `.mountDependency({ dependencyId, volumeId, subpath, mountpoint, readonly: true })` in the `Mounts` chain. A missing dependency volume makes the mount fail.

The normal init order is `restoreInit -> versionGraph -> setInterfaces -> actions -> dependencies -> setup`. Put `actions` before `dependencies` when `withInit` creates tasks for registered actions.
