# Depend on Another Service

When your service needs another StartOS service (e.g., a Bitcoin node for a wallet, or PostgreSQL from a shared instance), declare it as a dependency. You can require it to be installed, running, or healthy, and optionally pin a version range.

## Solution

In `setupDependencies()`, return an object mapping dependency package IDs to their requirements: `kind: 'running'` (the dependency should be running), `kind: 'exists'` (just installed), a `versionRange`, and `healthChecks` listing which of the dependency's daemons or standalone health checks the user should expect to be passing.

These declarations drive the **warning UI** StartOS shows the user when a dependency isn't installed, isn't running, or has a listed health check failing. They do **not** gate your service's startup — your service starts whenever the user starts it, regardless of dependency state. If your service genuinely cannot operate before a dependency reaches a particular state, handle that at runtime in `setupMain` (poll, retry, or surface your own error); don't expect the dependency declaration to block startup for you.

Read the dependency's connection info in `setupMain` by resolving its bridge address with `sdk.host.get(...).const()` — see [Service-to-Service Networking](service-to-service.md) for the one correct way (and the two forbidden ones: `.startos` DNS names and cross-package container IPs).

**Reference:** [Dependencies](dependencies.md) · [Service-to-Service Networking](service-to-service.md)

## Examples

See `startos/dependencies.ts` in: [electrs](https://github.com/Start9Labs/electrs-startos), [fulcrum](https://github.com/Start9Labs/fulcrum-startos), [jam](https://github.com/Start9Labs/jam-startos), [lightning-terminal](https://github.com/Start9Labs/lightning-terminal-startos), [lnbits](https://github.com/Start9Labs/lnbits-startos), [lnd](https://github.com/Start9Labs/lnd-startos), [mempool](https://github.com/Start9Labs/mempool-startos), [open-webui](https://github.com/Start9Labs/open-webui-startos), [public-pool](https://github.com/Start9Labs/public-pool-startos), [robosats](https://github.com/Start9Labs/robosats-startos), [bitcoin-explorer](https://github.com/Start9Labs/bitcoin-explorer-startos), [helipad](https://github.com/Start9Labs/helipad-startos), [cln](https://github.com/Start9Labs/cln-startos), [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos), [albyhub](https://github.com/Start9Labs/albyhub-startos), [immich](https://github.com/Start9Labs/immich-startos), [jellyfin](https://github.com/Start9Labs/jellyfin-startos), [start9-pages](https://github.com/Start9Labs/start9-pages-startos), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos)
