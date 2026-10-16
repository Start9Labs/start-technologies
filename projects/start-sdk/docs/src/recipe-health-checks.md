# Add Standalone Health Checks

Every daemon already includes a `ready` check that tells StartOS when it's started. Standalone health checks go beyond that — they monitor ongoing conditions like blockchain sync progress, network reachability, or secondary interface availability. These checks run continuously and are displayed to the user separately from daemon readiness.

## Solution

Use `.addHealthCheck()` on the daemon chain in `setupMain()`. Each health check has an ID, a `ready` function that returns a result, and a `requires` array specifying which daemons must be running first. The check function typically execs a CLI command or calls an API to assess the condition. Return `result: 'loading'` with a progress message for ongoing work (e.g. syncing), `result: 'success'` when complete, or `result: 'disabled'` when the check doesn't apply. Health check IDs are what dependency packages reference in their `healthChecks` array — a dependent service can require that your sync progress check passes before it considers your service ready.

For checks that call expensive RPCs or APIs, set a `trigger` to control polling frequency. The default polls every 1 s while pending, which can overload a service doing heavy work (block validation, indexing). Use `sdk.trigger.cooldownTrigger(ms)` for a fixed interval, or `sdk.trigger.statusTrigger(30_000, { starting: 5_000 })` for per-status intervals.

**Reference:** [Main](main.md) (Health Checks, Polling Triggers) · [Dependencies](dependencies.md)

## Examples

See `startos/main.ts` in: [bitcoin-core](https://github.com/Start9Labs/bitcoin-core-startos) (sync progress with trigger, I2P, Tor, clearnet reachability), [lnd](https://github.com/Start9Labs/lnd-startos) (sync progress, reachability), [cln](https://github.com/Start9Labs/cln-startos) (sync status), [electrs](https://github.com/Start9Labs/electrs-startos) (sync progress), [fulcrum](https://github.com/Start9Labs/fulcrum-startos) (sync progress), [monerod](https://github.com/Start9Labs/monerod-startos) (sync progress), [mempool](https://github.com/Start9Labs/mempool-startos) (sync), [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos) (UTXO sync), [synapse](https://github.com/Start9Labs/synapse-startos) (admin interface)
