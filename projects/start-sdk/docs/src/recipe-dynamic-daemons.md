# Create Dynamic Daemons

Some services need a variable number of daemons based on user configuration — one per tunnel, one per website, one per connected node. The daemon chain is built at runtime from a config list.

## Solution

Read a variable-length list from a file model in `setupMain()`, then loop over entries to build the daemon chain with `.addDaemon()`. All dynamic daemons can share a single subcontainer image. The daemon ID must be unique per entry — derive it from the entry's data. An alternative approach generates dynamic config files (e.g., nginx server blocks) from the list and runs a single daemon serving all entries.

Returning a plain `Daemons.of(...)` chain from `setupMain` rebuilds **every** daemon whenever the list changes (a reactive `.const(effects)` read re-runs `setupMain` via `effects.restart()`). When sub-instances are added, renamed, or removed at runtime and you don't want to bounce the unaffected ones — or restart the whole service — return **`sdk.Daemons.dynamic(effects, fn)`** instead. It reconciles the running set against a freshly-built one, touching only what actually changed.

> [!IMPORTANT]
> `main` is **always** `sdk.setupMain(...)`. What varies is what you return from it: a static `sdk.Daemons.of(...)` chain, or the reconciler from `sdk.Daemons.dynamic(effects, ...)`. Both are a `DaemonBuildable`. Never use `Daemons.dynamic` _as_ your `main` export.

```typescript
import { sdk } from './sdk'
import { tunnelsFile } from './fileModels/tunnels.json'

export const main = sdk.setupMain(async ({ effects }) => {
  // Return the reconciler — it is a DaemonBuildable, just like Daemons.of(...).
  return sdk.Daemons.dynamic(effects, async ({ effects }) => {
    // Re-runs whenever the watched file changes (a constRetry trigger).
    // Inside the builder, `constRetry` reconciles in place — it does NOT
    // restart the service.
    const tunnels = (await tunnelsFile.read().const(effects)) ?? []

    let daemons = sdk.Daemons.of(effects)
    for (const t of tunnels) {
      daemons = daemons.addDaemon(`tunnel-${t.id}`, {
        // Must be a LAZY SubContainer (`.of`, not `.eager`): the reconciler
        // rejects eager handles, and lazy ones are never materialized for
        // daemons that diff to "leave alone".
        subcontainer: sdk.SubContainer.of(effects, { imageId: 'tunnel' }, sdk.Mounts.of(), `tunnel-${t.id}`),
        exec: { command: ['tunnel', '--port', String(t.port)] },
        requires: [],
      })
    }
    return daemons // return the record-mode chain — do NOT call `.build()`.
  })
})
```

On each run the reconciler diffs entries by `id` and a `configHash` of their structural fields (`imageId`, `sharedRun`, `name`, `mounts`, `exec`, `requires`, and `ready`'s `display`/`gracePeriod`):

- **absent → present** — start the new daemon
- **present → absent** — stop the removed daemon
- **same `configHash`** — leave it running, untouched
- **different `configHash`** — restart it

Dependents (via `requires`) of any restarted or stopped daemon restart too, so the wiring stays consistent. Closures — `ready.fn`, `ready.trigger`, a function-form `exec.fn` — are **not** part of the hash. If such a closure captures a value the reconciler must react to (say, a host the `exec.fn` reads before building its command), pass that value as the entry's `uses`; any change to it restarts the daemon on the next reconcile. Only JSON-serializable `uses` values are useful — functions, symbols, cycles and `undefined` normalize to distinct `UNSERIALIZABLE:*` sentinels, so a change only visible there never triggers a restart, and BigInts hash as their decimal string.

**Reference:** [Main](main.md) · [Actions](actions.md) · [File Models](file-models.md)

## Examples

See `startos/main.ts` in: [holesail](https://github.com/Start9Labs/holesail-startos) (one daemon per tunnel), [start9-pages](https://github.com/Start9Labs/start9-pages-startos) (dynamic nginx config per website)
