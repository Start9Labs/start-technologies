# Marketplace

The Marketplace is where you discover services to install on your StartOS server. It is a collection of independent registries, like stores in a mall, each offering its own curated list of services.

## Registry System and Navigation

Each registry in the Marketplace is maintained independently. When you open the Marketplace, you browse one registry at a time. To switch between registries, open the registry selector above the search field. A registry that declares a description shows it above its services.

StartOS ships with two [default registries](default-registries.md): the **Start9 Registry** and the **Community Registry**. You can also add [alternative registries](alternative-registries.md) or [sideload](sideloading.md) packages directly.

## Package Signers

Package details show **Signed by** above **Version**, listing the enabled keyrings whose keys sign the selected package. The install button is disabled when no trusted signer matches. Manage [trusted keyrings](trusted-keys.md) under **System → Trusted keyrings**.

## Download Transports

When a registry advertises a BitTorrent source, StartOS prefers it for package downloads and OS updates, with HTTP fallback if the torrent is unavailable, stalls, or fails verification. Both transports use the same signed content commitments. BitTorrent sources must contain a single asset file.

> [!WARNING]
> BitTorrent peers can see the server's IP address and the torrent it requests. Onion registries use HTTP through Tor instead of BitTorrent.

To use HTTP exclusively, set `bittorrent: false` in `/media/startos/config/config.yaml` and restart StartOS.

### Optional Seeding

Completed, verified BitTorrent downloads can be retained for seeding after installation or an update. Add the following to `/media/startos/config/config.yaml` and restart StartOS:

```yaml
bittorrent-seed:
  directory: /media/startos/data/registry-seeds
```

StartOS keeps separate copies of these downloads and resumes seeding them after a restart. The cache grows as assets are retained. To reclaim it, remove the `bittorrent-seed` setting, restart StartOS, then delete the dedicated cache directory or selected entries inside it.

Seeding uses outgoing peer connections by default. To accept incoming connections, add `listen: '[::]:6882'` under `bittorrent-seed` and arrange TCP reachability for that address and port. Router and firewall configuration remain the administrator's responsibility. Disabling BitTorrent also disables retained seeding.

## Open Ecosystem

Anyone, anywhere can establish their own registry. This open model means no single entity controls which services are available, and no service can be effectively censored. Users choose which signing keys to trust independently of the hosting registry, and services can be distributed through multiple independent registries, direct file transfer, or built from source.
