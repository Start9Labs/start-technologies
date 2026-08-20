# Handle Version Upgrades

When you release a new version of your package, users upgrading from older versions may need data migrations — transforming config formats, moving files, or updating store schemas. The version graph defines the migration path between versions.

This is the mechanism for anything keyed to the package version — the data on disk was written by an older release and the new one cannot read it as-is. It runs once per install, covers restoring a backup taken below the current version, and never runs on a fresh install. Work whose answer can differ on the next start belongs in a oneshot instead; see [main.md § Choosing Between a Oneshot, an Init, and a Migration](main.md#choosing-between-a-oneshot-an-init-and-a-migration).

## Solution

Define a `VersionGraph` with a `current` version and an array of `other` (previous) versions. Each version has `up` and `down` migration functions. Use `IMPOSSIBLE` for directions that can't be migrated. The `up` migration transforms old config, moves files, or runs `storeJson.merge(effects, {})` to apply new zod defaults. Only versions that introduced a migration need entries in the `other` array — `VersionGraph` reaches every other prior version on its own.

The latest version always lives in `startos/versions/current.ts`. You create a new file when the version **already in** `current.ts` carries a migration, because a migration stays with the version that introduced it and is never carried forward: rename the existing `current.ts` to the version it holds (e.g. `v2.3.2_1.ts`), add that version to `other`, then write a fresh `current.ts` carrying the new version and whatever migration it needs of its own. Bump in place only when the outgoing version's migration is empty. See [Versions — When to Create a New Version File](versions.md#when-to-create-a-new-version-file).

**Reference:** [Versions](versions.md) · [File Models](file-models.md)

A version that stops binding a host or a port has one more job: retire it in the same `up()`, or its external port stays claimed and dependencies keep resolving a dead address. See [Retiring a Host or Binding](interfaces.md#retiring-a-host-or-binding).

## Examples

See `startos/versions/` in: [bitcoin-core](https://github.com/Start9Labs/bitcoin-core-startos), [cln](https://github.com/Start9Labs/cln-startos), [lnd](https://github.com/Start9Labs/lnd-startos), [monerod](https://github.com/Start9Labs/monerod-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [simplex](https://github.com/Start9Labs/simplex-startos), [tor](https://github.com/Start9Labs/tor-startos), [synapse](https://github.com/Start9Labs/synapse-startos)
