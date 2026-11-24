# Mount Volumes from Another Service

Some services need read-only access to files from another service — media files from a file manager, credentials a node publishes for its dependents. Mount a dependency's volume into your container.

## Solution

In the `Mounts` chain in `setupMain()`, use `.mountDependency()` typed against the dependency's manifest. Specify the dependency's `volumeId`, a `subpath` (or `null` for the whole volume), a `mountpoint` in your container, and `readonly: true`. In `dependencies.ts`, declare the dependency with `kind: 'exists'` (if you just need the files) or `kind: 'running'` (if the dependency must be active).

**Mount the narrowest thing the dependency publishes for dependents, never its data volume to reach one file in it.** `readonly` stops writes, not reads: everything on the mount — wallets, seeds, its `store.json`, the credentials it holds for other services — is readable by your image, and a Unix socket on it still gives you the full RPC. For a credential (an RPC cookie, macaroon, rune), mount the tier on the dependency's `public` volume that grants only what you need (`volumeId: 'public', subpath: 'readonly'`); if the dependency publishes none, add one to it first (see [Publishing credentials for dependents](dependencies.md#publishing-credentials-for-dependents)). Read the credential where it is mounted. Don't copy it onto your own volume or into your config file, where every backup of your service carries it; fix an ownership mismatch with [`idmap`](main.md#remapping-ownership-idmap) on the mount, and hand the app a credential it wants inline through an environment variable.

A read-only mount of a `subpath` that doesn't exist fails, which makes a throwaway one an existence check: mount it in `sdk.SubContainer.withTemp()` and treat a failure as missing. Only a read-only mount works for this — a read-write one creates the missing folder on the dependency's volume and succeeds.

**Reference:** [Dependencies](dependencies.md) · [Main](main.md)

## Examples

See `startos/main.ts` and `startos/dependencies.ts` in: [jellyfin](https://github.com/Start9Labs/jellyfin-startos) (NextExplorer + FileBrowser Quantum + Nextcloud media), [helipad](https://github.com/Start9Labs/helipad-startos) (LND macaroons/certs), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos) (LND + CLN + Eclair volumes), [lightning-terminal](https://github.com/Start9Labs/lightning-terminal-startos) (LND certs), [albyhub](https://github.com/Start9Labs/albyhub-startos) (LND, CLN, or phoenixd volume)

For the existence check, see `canMount` in [audiobookshelf](https://github.com/Start9Labs/audiobookshelf-startos)'s `startos/utils.ts` and `checkSubpathExists` in [navidrome](https://github.com/Start9-Community/navidrome-startos)'s `startos/actions/mediaSources.ts`.
