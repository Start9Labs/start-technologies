# Support Alternative Dependencies

Some services can work with multiple backends — LND or Core Lightning for Lightning, File Browser or Nextcloud for media. An action lets the user choose, and `setupDependencies` reads that choice to declare only the selected dependency.

## Solution

Create a selection action with `Value.select()` that lets the user choose between backends (e.g., LND vs CLN). Persist the choice to **`store.json` on the `startos` volume** — it is StartOS-level state, not part of the upstream service's config, so it must not be an invented key in the app's config file. In `setupDependencies()`, read the choice and conditionally return only the selected dependency. In `setupMain()`/`init`, read the same choice to conditionally mount the selected dependency's volumes and resolve _that_ backend's bridge address into the appropriate env vars or config keys.

**Reference:** [Dependencies](dependencies.md) · [Service-to-Service Networking](service-to-service.md#state-that-a-config-value-is-derived-from) · [Actions](actions.md) · [Main](main.md)

## Examples

See `startos/dependencies.ts` and `startos/actions/` in: [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos) (LND/CLN/Monero), [lnbits](https://github.com/Start9Labs/lnbits-startos) (LND/CLN), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos) (LND + CLN + remote nodes), [jellyfin](https://github.com/Start9Labs/jellyfin-startos) (File Browser/Nextcloud), [mempool](https://github.com/Start9Labs/mempool-startos) (Fulcrum/Electrs + LND/CLN), [albyhub](https://github.com/Start9Labs/albyhub-startos) (LND/LDK)
