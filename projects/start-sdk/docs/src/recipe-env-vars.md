# Pass Config via Environment Variables

Some services expect configuration through environment variables rather than config files. StartOS lets you set them in the daemon's `exec.env` object, with values sourced from file models, store.json, or hardcoded strings.

## Solution

Read values from file models or store.json in `setupMain()`, then pass them as the `env` property of the daemon's `exec` config. Values can be read reactively with `.const(effects)` so the daemon restarts when config changes. Hardcoded values like ports and feature flags can be set as plain strings directly in the `env` object.

**Reference:** [Main](main.md) · [File Models](file-models.md)

## Examples

See `startos/main.ts` in: [ghost](https://github.com/Start9Labs/ghost-startos), [gitea](https://github.com/Start9Labs/gitea-startos), [immich](https://github.com/Start9Labs/immich-startos), [lnbits](https://github.com/Start9Labs/lnbits-startos), [mempool](https://github.com/Start9Labs/mempool-startos), [spliit](https://github.com/Start9Labs/spliit-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos), [open-webui](https://github.com/Start9Labs/open-webui-startos), [searxng](https://github.com/Start9Labs/searxng-startos), [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos), [bitcoin-explorer](https://github.com/Start9Labs/bitcoin-explorer-startos), [helipad](https://github.com/Start9Labs/helipad-startos), [albyhub](https://github.com/Start9Labs/albyhub-startos), [jam](https://github.com/Start9Labs/jam-startos), [jitsi](https://github.com/Start9Labs/jitsi-startos), [ollama](https://github.com/Start9Labs/ollama-startos), [public-pool](https://github.com/Start9Labs/public-pool-startos), [robosats](https://github.com/Start9Labs/robosats-startos), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos)
