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

BitTorrent downloads buffer a complete asset on the data disk before unpacking package files or applying an OS update. Install progress appears once signed package metadata is ready, and the remaining transfer can be cancelled. Metadata can come from advertised HTTP mirrors or BitTorrent peers. Temporary transfer files are cleaned up afterward.

> [!WARNING]
> BitTorrent peers can see the server's IP address and the torrent it requests. Onion registries use HTTP through Tor instead of BitTorrent.

To use HTTP exclusively, set `bittorrent: false` in `/media/startos/config/config.yaml` and restart StartOS.

### Software Sharing

Sharing compatible installed S9PKs and OS images already stored by StartOS is disabled by default. Enable **Share S9PKs and OS images** under **System → Software sharing**, or use:

```sh
start-cli server bittorrent set-seeding --enabled true
```

This preference applies without restarting and persists across restarts. Active BitTorrent downloads can upload even when it is off. Use `--enabled false` to stop sharing stored software.

StartOS discovers advertisements for installed packages from their recorded registry and for stored OS images from the selected update registry. It also remembers torrent metadata from new installations and updates, including downloads that fall back to HTTP. Files without an advertised source are not assigned a swarm.

Sharing requires an exact byte match with the advertised torrent. Installation can filter package architectures and remove supplemental signature trailers, leaving an installed S9PK that cannot join the original swarm. StartOS skips incompatible files without modifying them.

Sharing adds only source-path, commitment and torrent-metadata records under `/media/startos/data/package-data/registry-torrents`. It creates no extra payload copies or extended retention: installation, uninstall and native image cleanup still control the software files. Disabling sharing keeps this ordinary file management unchanged.

Uploads can use outgoing peer connections without an incoming port forward. To accept incoming peers, configure the following in `/media/startos/config/config.yaml`, restart StartOS, and allow TCP traffic to that address and port through the firewall and, where needed, a router port forward:

```yaml
bittorrent-seed:
  listen: '[::]:6882'
```

An optional `directory` under `bittorrent-seed` overrides the metadata-catalog location, not a payload-storage location. These storage/listener options do not enable sharing. StartOS does not create an automatic UPnP port mapping; router and firewall configuration remain the administrator's responsibility. `bittorrent: false` suppresses both peer downloads and software sharing while preserving the saved sharing preference.

## Open Ecosystem

Anyone, anywhere can establish their own registry. This open model means no single entity controls which services are available, and no service can be effectively censored. Users choose which signing keys to trust independently of the hosting registry, and services can be distributed through multiple independent registries, direct file transfer, or built from source.
