# Service-to-Service Networking

[Interfaces](interfaces.md) covers how your service exposes ports **inbound**. This page covers the reverse: how your service reaches **another** service at runtime — a wallet dialing Bitcoin's RPC, an indexer dialing Bitcoin's P2P port, anything dialing Tor's SOCKS proxy.

There is exactly one supported way to do this, and two once-common patterns that are now forbidden.

## The host bridge

Every StartOS service runs in its own container on a single internal bridge, `lxcbr0`. The bridge gateway — the OS itself — always sits at a fixed address you read with:

```typescript
const osIp = await sdk.getOsIp(effects) // "10.0.3.1" — a plain Promise, not reactive
```

`getOsIp` is a one-shot read of a compile-time constant. It never changes and never needs watching.

Every port a service **binds** (via `sdk.MultiHost.of(...).bindPort(...)` in [interfaces.ts](interfaces.md)) is reachable from other containers at `10.0.3.1:<assigned external port>`. This holds even for a binding with **no exported interface** — binding a port is enough to make it reachable on the bridge (see [Exposing a bridge-only port](#exposing-a-bridge-only-port) below).

> [!IMPORTANT]
> The bridge exposes a service at its **assigned external port**, which you must not assume. `preferredExternalPort` is a request, not a guarantee: the first service to claim a given external port gets it and later claimants fall back to a random port, and when `preferredExternalPort` is omitted it defaults to the protocol default (`http` → 80). So the external port is only knowable at runtime, by reading the dependency's live binding. What *is* stable is the dependency's **internal** port and **host id** — import those as constants from the dependency's package.

## Reaching a dependency

Resolve a dependency's bridge address by reading its host and mapping to `<osIp>:<assignedPort>`:

```typescript
const osIp = await sdk.getOsIp(effects)

const rpcAddr = await sdk.host
  .get(effects, { packageId: 'bitcoind', hostId: rpcHostId }, (host) => {
    const port = host?.bindings[rpcInternalPort]?.net.assignedPort
    return port != null ? `${osIp}:${port}` : null
  })
  .const()
```

Three things make this correct, and each matters:

1. **Read `net.assignedPort`, keyed by the dependency's internal port** — not an `addressInfo` hostname. `assignedPort` is the minimal thing that identifies the bridge address, and it changes *only* when the external port changes. A binding's `addressInfo` hostname list, by contrast, folds in things that have nothing to do with reaching it over the bridge — the box's LAN IP changing, a Tor or clearnet address being added or removed — any of which would change a hostname-derived value and needlessly restart your service. `osIp` + `assignedPort` is stable against all of that. `rpcHostId` and `rpcInternalPort` are imported from the dependency's package (`bitcoin-core-startos/startos/utils`), not hardcoded — the internal port and host id are the stable contract.

2. **`.const()` on a minimal mapped value.** `.const()` re-runs `main` only when the **mapped** value changes, not on every churn of the dependency's host record. Because the mapped value is just the address string, the restart behavior is exactly what you want:

   | Event | Restarts your service? |
   |-------|------------------------|
   | Dependency updated | **No** — its assigned port survives an update |
   | Dependency installed *after* yours | Once, to heal onto the now-resolvable address |
   | Dependency uninstalled | Once, to reconfigure to the absent state |
   | Dependency reinstalled, same port | No |
   | Dependency reinstalled, new port | Once, to heal |
   | Conditional binding appears (e.g. LND unlock) | Once, then stable across lock/unlock |

   Do **not** use `.once()` (it snapshots `null` forever if the dependency isn't installed yet — your service never heals when the user installs the dependency second) or `.waitFor()` (it blocks `main` before any daemon or health check exists, leaving the service stuck "starting" with no signal). `.const()` on the minimal value is the only option that both avoids needless restarts and self-heals.

3. **Absent means absent — never fabricate an address.** When the map returns `null` (dependency not installed), write *nothing* for that dependency: leave the config key out, make the file-model field `.optional().catch(undefined)`, omit the env var. Let the dial fail and the health check go red. **Never** write a placeholder like `127.0.0.1:8332` for a cross-container dependency — that address can't reach the dependency's container and only masks the real state. The `.const()` heals the moment the dependency appears.

### A reusable helper

Packages wrap the pattern above in a small `utils.ts` helper so each call site stays a one-liner. This is the fleet convention and a drop-in for the planned `sdk.host.getBridgeAddress`:

```typescript
export function bridgeAddress(
  effects: T.Effects,
  opts: { packageId: string; hostId: string; internalPort: number; fallbackPort?: number },
) {
  const watchable = async () => {
    const osIp = await sdk.getOsIp(effects)
    return sdk.host.get(effects, { packageId: opts.packageId, hostId: opts.hostId }, (host) => {
      const port = host?.bindings[opts.internalPort]?.net.assignedPort ?? opts.fallbackPort
      if (port == null) return null
      return `${osIp}:${port}`
    })
  }
  return {
    const: async () => (await watchable()).const(), // reactive, in main / init
    once: async () => (await watchable()).once(),   // snapshot, in an action
  }
}
```

Use `.const()` in `setupMain` and `setupOnInit`; use `.once()` only inside an action, where a live snapshot (not a subscription) is what you want.

## The Tor exception: always-on flags

Some flags should be passed **unconditionally**, even when the dependency is absent — most commonly Bitcoin's `-onion=<tor SOCKS>`. A dead bridge address there is harmless (connection refused), and passing the flag always means Tor works the moment it's installed with no reconfiguration.

For this, and only this, use `fallbackPort` so the value is never `null`:

```typescript
const torSocks = await bridgeAddress(effects, {
  packageId: 'tor',
  hostId: socksHostId,
  internalPort: socksPort, // 9050
  fallbackPort: socksPort, // keeps the value at `${osIp}:9050` when Tor is absent
}).const()
```

Tor's SOCKS port (9050) is the one external port StartOS guarantees is claimable, so `${osIp}:9050` is always valid. This is not a license to fabricate addresses generally (see rule 3 above) — it applies to Tor's SOCKS proxy, whose address is fixed and whose flag is inert when unreachable.

Track a dependency's *presence* (for a health check, say) with `sdk.getStatus(effects, { packageId }).onChange(...)`, registered **unconditionally** — it returns `null` when the dependency is uninstalled and re-fires when it's installed. Never gate the watch itself behind a startup-time presence check.

## State that a config value is derived from

Sometimes an address depends on a choice the user made — which of several interchangeable backends to use (Fulcrum vs. Electrs, LND vs. CLN). That choice is **StartOS-level state**. It belongs in your package's own `store.json` — **never** as an invented key in the upstream service's config file, which may contain only keys the upstream software recognizes.

A package keeps StartOS state in a single `store.json` file model (see [File Models](file-models.md)). If your package has no other on-disk state to colocate it with, put it on the dedicated `startos` volume:

```typescript
// store.json.ts — StartOS state, kept out of the upstream config
const shape = z.object({
  indexer: z.enum(['electrs', 'fulcrum']).optional().catch(undefined),
})
export const storeJson = FileHelper.json(
  { base: sdk.volumes.startos, subpath: '/store.json' },
  shape,
)
```

Declare the volume in the manifest (`volumes: [..., 'startos']`) and add it to the backup set if the choice must survive a restore. `setupDependencies` and the selection action read/write `store.json`; `init` reads the choice, resolves *that* backend's bridge address, and writes only the real upstream keys into the app config. It is a bug to add a discriminator field (`INDEXER`, `BACKEND_CHOICE`, …) to a file model that maps the upstream service's own config file.

## Exposing a bridge-only port

If you are on the *provider* side — you want other services to reach a port but you do **not** want it on the LAN — bind the port and simply don't export an interface on it. A binding with no exported interface is reachable on `lo`/`lxcbr0` only, never the LAN. This is how the Tor service publishes its SOCKS proxy:

```typescript
// tor-startos/startos/interfaces.ts
await sdk.MultiHost.of(effects, socksHostId).bindPort(socksPort, {
  protocol: null,
  preferredExternalPort: socksPort,
  addSsl: null,
  secure: { ssl: false },
})
// no origin.export([...]) — bridge/lo only, off the LAN
```

Export the host id and internal port as constants so dependents import them rather than hardcoding.

## Forbidden patterns

Two patterns that older packages used are being removed. Do not introduce them:

- **`<package-id>.startos` DNS names** (`http://bitcoind.startos:8332`). The overlay DNS that resolved these is deprecated and will be removed. Resolve the bridge address instead.
- **Cross-package container IPs** (`sdk.getContainerIp(effects, { packageId })`). A dependency's container IP is not stable across its restarts/updates and reading it reactively restarts your service on every dependency churn. Use the bridge. (`getContainerIp` with **no** `packageId` — your *own* container IP — remains fine.)

## Reference implementations

- **`bitcoin-core`** — `startos/utils.ts` (`bridgeAddress` helper) and `startos/main.ts` (`torSocks` with the `fallbackPort` case).
- **`lnd`** — `startos/utils.ts` resolves bitcoind's RPC and ZMQ hosts; a conditional (unlock-gated) binding handled by the same `.const()` pattern.
- **`mempool`** — `startos/utils.ts` + `startos/file-models/store.json.ts`: the backend-selection choice in `store.json`, the resolved address written to the upstream config's real keys.
