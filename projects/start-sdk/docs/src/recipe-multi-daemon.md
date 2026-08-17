# Run Multiple Containers

Complex services often need multiple processes — an application server plus a database, a web frontend plus a backend API, or an app plus a cache layer. Each container gets its own subcontainer, daemon definition, health check, and dependency chain.

## Solution

Create multiple `SubContainer` instances in `setupMain()` — one per image (e.g., app, database, cache). Chain `.addDaemon()` calls for each. Use the `requires` array to control startup order — daemons wait for their dependencies' health checks to pass before starting. Each daemon gets its own volume mounts, env vars, and health check.

**Reference:** [Main](main.md)

## Examples

See `startos/main.ts` in: [am-i-exposed](https://github.com/Start9Labs/am-i-exposed-startos), [bitcoin-core](https://github.com/Start9Labs/bitcoin-core-startos), [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos), [cln](https://github.com/Start9Labs/cln-startos), [ghost](https://github.com/Start9Labs/ghost-startos), [immich](https://github.com/Start9Labs/immich-startos), [jitsi](https://github.com/Start9Labs/jitsi-startos), [mempool](https://github.com/Start9Labs/mempool-startos), [monerod](https://github.com/Start9Labs/monerod-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [searxng](https://github.com/Start9Labs/searxng-startos), [simplex](https://github.com/Start9Labs/simplex-startos), [spliit](https://github.com/Start9Labs/spliit-startos), [synapse](https://github.com/Start9Labs/synapse-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos), [bitcoin-explorer](https://github.com/Start9Labs/bitcoin-explorer-startos)
