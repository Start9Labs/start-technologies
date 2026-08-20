# Run a Redis/Valkey Cache

Caching layers improve performance for web applications. Valkey (Redis-compatible) runs as a sidecar daemon with no persistent storage — purely ephemeral.

## Solution

Add a Valkey daemon with no persistent volume (ephemeral cache). Disable persistence with `--save '' --appendonly no`. Health-check by execing `valkey-cli ping` and comparing stdout to `"PONG"`. Use `display: null` to hide the check from the user since it's an internal implementation detail. The app daemon declares `requires: ['valkey']` to start after the cache is ready.

**Reference:** [Main](main.md)

## Examples

See `startos/main.ts` in: [immich](https://github.com/Start9Labs/immich-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [searxng](https://github.com/Start9Labs/searxng-startos), [bitcoin-explorer](https://github.com/Start9Labs/bitcoin-explorer-startos)
