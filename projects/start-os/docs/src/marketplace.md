# Marketplace

The Marketplace is where you discover services to install on your StartOS server. It is a collection of independent registries, like stores in a mall, each offering its own curated list of services.

## Registry System and Navigation

Each registry in the Marketplace is maintained independently. When you open the Marketplace, you browse one registry at a time. To switch between registries, open the registry selector above the search field. Its gear opens **System → Registries**. A registry that declares a description shows it above its services.

StartOS ships with two [default registries](default-registries.md): the **Start9 Registry** and the **Community Registry**. You can also add [alternative registries](alternative-registries.md) or [sideload](sideloading.md) packages directly.

## Managing Registries

**System → Registries** lists your registries. The one that provides StartOS updates carries a **StartOS updates** badge. Click **Add registry** and enter a registry's URL to add it; the keys it offers that your server does not already have become trusted for packages, and StartOS lists them. A registry's menu offers:

- **Manage keys** opens [Trusted Keys](trusted-keys.md) filtered to the keys the registry offers.
- **Use for StartOS updates** makes the registry the source of StartOS updates and trusts its keys for StartOS updates. StartOS first checks that the registry publishes StartOS updates, and warns before using any registry other than the Start9 Registry.
- **Remove** removes the registry. Afterward, StartOS offers to remove the keys it added that no connected registry still offers. The Start9 Registry and the Community Registry cannot be removed, and neither can the registry that provides StartOS updates until you choose another.

## Package Signers

Package details include a **Signed by** section listing every key that signed the package: a green check for a key you trust for packages, a red X for a key you know but do not trust, and a gray dash with the key's fingerprint for a key your server does not know. Select a key to see its details and trust or distrust it for packages or StartOS updates; an unknown key needs a label first. Confirm a key's fingerprint through a source you already trust before trusting it. When no trusted signer matches, a warning appears, and installing asks you to confirm the risks first. Manage individual keys and their package and StartOS-update trust under **System → [Trusted Keys](trusted-keys.md)**.

## Download Transports

When a registry advertises a BitTorrent source, StartOS prefers it for package downloads and OS updates, with HTTP fallback if the torrent is unavailable, stalls, or fails verification. Both transports use the same signed content commitments. BitTorrent sources must contain a single asset file. V1/v2 hybrid torrents use their v1-compatible swarm. Recovery-mode OS updates stream over HTTP directly to the image partition.

BitTorrent downloads buffer a complete asset on the data disk before installation. Retained seeding keeps an additional copy.

> [!WARNING]
> BitTorrent peers can see the server's IP address and the torrent it requests. Onion registries use HTTP through Tor instead of BitTorrent.

To use HTTP exclusively, set `bittorrent: false` in `/media/startos/config/config.yaml` and restart StartOS.

### Optional Seeding

Completed, verified BitTorrent downloads can be retained for seeding after installation or an update. Add the following to `/media/startos/config/config.yaml` and restart StartOS:

```yaml
bittorrent-seed:
  directory: /media/startos/data/package-data/registry-seeds
```

StartOS keeps separate copies of these downloads and resumes seeding them after a restart. The cache grows as assets are retained. To reclaim it, remove the `bittorrent-seed` setting, restart StartOS, then delete the dedicated cache directory or selected entries inside it.

Seeding uses outgoing peer connections by default. To accept incoming connections, add `listen: '[::]:6882'` under `bittorrent-seed` and arrange TCP reachability for that address and port. Router and firewall configuration remain the administrator's responsibility. Disabling BitTorrent also disables retained seeding.

## Open Ecosystem

Anyone, anywhere can establish their own registry. This open model means no single entity controls which services are available, and no service can be effectively censored. Users choose which signing keys to trust independently of the hosting registry, and services can be distributed through multiple independent registries, direct file transfer, or built from source.
