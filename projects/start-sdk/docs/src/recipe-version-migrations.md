# Handle Version Upgrades

When you release a new version of your package, users upgrading from older versions may need data migrations — transforming config formats, moving files, or updating store schemas. The version graph defines the migration path between versions.

## Solution

Define a `VersionGraph` with a `current` version and an array of `other` (previous) versions. Each version has `up` and `down` migration functions. Use `IMPOSSIBLE` for directions that can't be migrated. The `up` migration transforms old config, moves files, or runs `storeJson.merge(effects, {})` to apply new zod defaults. Only versions that users might be upgrading _from_ need entries in the `other` array.

The latest version always lives in `startos/versions/current.ts`. Adding a migration is the one case where you create a new file: rename the existing `current.ts` to the version it holds (e.g. `v2.3.2_1.ts`), add that version to `other`, then write a fresh `current.ts` carrying the new version and its `up`/`down` migration. Bumps that need no migration just edit `current.ts` in place. See [Versions — When to Create a New Version File](versions.md#when-to-create-a-new-version-file).

**Reference:** [Versions](versions.md) · [File Models](file-models.md)

## Examples

See `startos/versions/` in: [bitcoin-core](https://github.com/Start9Labs/bitcoin-core-startos), [cln](https://github.com/Start9Labs/cln-startos), [lnd](https://github.com/Start9Labs/lnd-startos), [monerod](https://github.com/Start9Labs/monerod-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [simplex](https://github.com/Start9Labs/simplex-startos), [tor](https://github.com/Start9Labs/tor-startos), [synapse](https://github.com/Start9Labs/synapse-startos)
