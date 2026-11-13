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

For a provider with the dependency's own package ID, a range is compared against its installed version and its `satisfies` list. For a different package ID, the range is compared against its explicit `satisfiesPackages` claims for the dependency ID. One version must satisfy a complete `&&` conjunction; `||` branches may match different declared versions. Negated ranges and `!=` veto a matching branch.

## Compatible Providers

A package can provide the complete contract of another package. Declare the exact ExVer versions it replaces on the current `VersionInfo`:

```typescript
import { VersionInfo } from '@start9labs/start-sdk'

export const current = VersionInfo.of({
  version: '5.0.0:0',
  releaseNotes: 'Initial release',
  migrations: {},
})
  .satisfies('4.0.0:0')
  .satisfies('bitcoind', '28.0:0')
  .satisfies('bitcoind', '29.0:0')
```

The one-argument overload declares compatibility with a version of the package itself; it populates `satisfies`. The two-argument overload declares compatibility with a version of the named package; it accumulates `satisfiesPackages`, a mapping of package IDs to version lists. Both take concrete ExVer versions, not ranges. The named package must differ from `manifest.id`; the builder rejects self-qualified claims. Use the one-argument overload for same-package aliases. `buildManifest` publishes the current version's claims; historical version declarations do not carry forward automatically. Cross-package claims raise the manifest's minimum StartOS version to 0.4.3. Provider discovery requires Start Registry 1.6.0 or later.

A cross-package claim promises the named package version's **complete dependency contract**: the same host, service-interface, volume, action, and health-check IDs and their behavior, including action inputs and results. Matching a network protocol alone is not sufficient. Dependents continue using the declared dependency ID; StartOS routes dependency checks, addresses and interfaces, volume mounts, actions and tasks to the selected provider.

Claims are **direct**, not transitive. If A claims B and B claims C, A must explicitly claim C to provide it. A provider's own version and its one-argument `satisfies` entries never count toward another package ID, even when their version numbers match.

Provider selection belongs to each dependent service, not to the server globally. StartOS automatically selects a unique compatible installed provider. Choices are unlocked by default. A selected provider stays selected while compatible, even when another compatible provider is installed. If an unlocked selection becomes incompatible or unavailable, StartOS falls back to a sole compatible provider. With multiple compatible candidates and no compatible selection, the user must select a provider through **Switch** on the dependent's dependency card; with none, the dependency is unsatisfied. An unlocked explicit choice remains preferred and returns when compatible again.

Locking pins the exact provider ID and forbids fallback: removing that provider or withdrawing compatibility leaves the dependency unsatisfied. The saved pin retains an embedded icon and title after uninstall, and the Marketplace link filters to that exact package. The **Switch** dialog contains the lock control. A sole locked choice shows **Unlock** directly on the dependency card, including an unavailable pin without alternatives; an unlocked sole provider or a missing pin with a compatible alternative shows **Switch**. Dialog confirmation applies the selected provider and lock together; cancellation preserves both. Unlocking restores fallback. Explicitly selecting another provider overrides any existing lock and starts unlocked. Changing the effective provider rebuilds the dependent service before it uses the replacement's resources.

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

Resolve its live address with `sdk.host.getBridgeAddress`, as described in [Service-to-Service Networking](service-to-service.md). For a volume mount, use `.mountDependency({ dependencyId, volumeId, subpath, mountpoint, readonly: true })` in the `Mounts` chain. A missing dependency volume makes the mount fail, and so does a missing `subpath` on a read-only mount; a read-write mount creates the missing `subpath` as a folder on the dependency's volume.

The normal init order is `restoreInit -> versionGraph -> setInterfaces -> actions -> dependencies -> setup`. Put `actions` before `dependencies` when `withInit` creates tasks for registered actions.
