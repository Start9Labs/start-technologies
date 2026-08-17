# Bootstrap via Temporary Daemon Chain

Some services can only be configured through their own API — they have no CLI for initial setup. During install, you need to start the service temporarily, call its API to bootstrap (create admin users, set config, register apps), then shut everything down before normal startup. The `runUntilSuccess` pattern handles this.

## Solution

In `setupOnInit` (on install), build a daemon chain with `.addDaemon()` and `.addOneshot()` just like in `setupMain()`, then call `.runUntilSuccess(timeout)` instead of returning the chain. The daemon starts, its health check passes, then the dependent oneshot runs the bootstrap logic (typically HTTP calls to the service's API). Once the oneshot completes successfully, all processes are cleaned up automatically. The timeout (in milliseconds) controls how long to wait before giving up.

**Reference:** [Initialization](init.md) · [Main](main.md)

## Examples

See `startos/init/` in: [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [actual-budget](https://github.com/Start9Labs/actual-budget-startos), [immich](https://github.com/Start9Labs/immich-startos), [garage](https://github.com/Start9Labs/garage-startos)
