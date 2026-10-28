# Mount Volumes from Another Service

Some services need read-only access to files from another service — media files from a file manager, TLS certificates from a Lightning node, or shared data directories. Mount a dependency's volume into your container.

## Solution

In the `Mounts` chain in `setupMain()`, use `.mountDependency()` typed against the dependency's manifest. Specify the dependency's `volumeId`, a `subpath` (or `null` for the whole volume), a `mountpoint` in your container, and `readonly: true`. In `dependencies.ts`, declare the dependency with `kind: 'exists'` (if you just need the files) or `kind: 'running'` (if the dependency must be active).

A read-only mount of a `subpath` that doesn't exist fails, which makes a throwaway one an existence check: mount it in `sdk.SubContainer.withTemp()` and treat a failure as missing. Only a read-only mount works for this — a read-write one creates the missing folder on the dependency's volume and succeeds.

**Reference:** [Dependencies](dependencies.md) · [Main](main.md)

## Examples

See `startos/main.ts` and `startos/dependencies.ts` in: [jellyfin](https://github.com/Start9Labs/jellyfin-startos) (NextExplorer + FileBrowser Quantum + Nextcloud media), [helipad](https://github.com/Start9Labs/helipad-startos) (LND macaroons/certs), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos) (LND + CLN + Eclair volumes), [lightning-terminal](https://github.com/Start9Labs/lightning-terminal-startos) (LND certs), [albyhub](https://github.com/Start9Labs/albyhub-startos) (LND, CLN, or phoenixd volume)

For the existence check, see `canMount` in [audiobookshelf](https://github.com/Start9Labs/audiobookshelf-startos)'s `startos/utils.ts` and `checkSubpathExists` in [navidrome](https://github.com/Start9-Community/navidrome-startos)'s `startos/actions/mediaSources.ts`.
