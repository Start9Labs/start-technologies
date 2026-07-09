# Generate Config Files

Most services read their configuration from files (YAML, TOML, INI, JSON, ENV). StartOS file models let you define the file's schema in zod, then read and write it type-safely. The schema doubles as the source of truth for defaults — use `.catch()` on every field so files self-heal and `merge()` works correctly.

## Solution

Define a `FileHelper` (`.json()`, `.yaml()`, `.toml()`, etc.) with a zod schema where every field has `.catch()` for self-healing defaults. Use `.merge()` to write (preserves unknown keys), `.read().const(effects)` for reactive reads that restart the daemon on change, and `.read().once()` for one-time reads. Seed defaults on install with `fileModel.merge(effects, {})` — the empty merge fills every missing field from its `.catch()` default. It does not strip or regenerate what's already there; see [What an Empty merge() Does](file-models.md#what-an-empty-merge-does).

**Reference:** [File Models](file-models.md) · [Main](main.md)

## Examples

See `startos/fileModels/` in: [bitcoin-core](https://github.com/Start9Labs/bitcoin-core-startos), [cln](https://github.com/Start9Labs/cln-startos), [lnd](https://github.com/Start9Labs/lnd-startos), [electrs](https://github.com/Start9Labs/electrs-startos), [fulcrum](https://github.com/Start9Labs/fulcrum-startos), [monerod](https://github.com/Start9Labs/monerod-startos), [nostr-rs-relay](https://github.com/Start9Labs/nostr-rs-relay-startos), [searxng](https://github.com/Start9Labs/searxng-startos), [synapse](https://github.com/Start9Labs/synapse-startos), [tor](https://github.com/Start9Labs/tor-startos), [simplex](https://github.com/Start9Labs/simplex-startos), [ghost](https://github.com/Start9Labs/ghost-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [home-assistant](https://github.com/Start9Labs/home-assistant-startos), [public-pool](https://github.com/Start9Labs/public-pool-startos), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos), [mcaptcha](https://github.com/Start9Labs/mcaptcha-startos), [bitcoin-explorer](https://github.com/Start9Labs/bitcoin-explorer-startos)
