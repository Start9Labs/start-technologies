# Marketplace

The Marketplace is where you discover services to install on your StartOS server. It is a collection of independent registries, like stores in a mall, each offering its own curated list of services.

## Registry System and Navigation

Each registry in the Marketplace is maintained independently. When you open the Marketplace, you browse one registry at a time. To switch between registries, open the registry selector above the search field. Its gear opens **System → Registries**. A registry that declares a description shows it above its services.

StartOS ships with two [default registries](default-registries.md): the **Start9 Registry** and the **Community Registry**. You can also add [alternative registries](alternative-registries.md) or [sideload](sideloading.md) packages directly.

## Managing Registries

**System → Registries** lists your registries. The one that provides StartOS updates carries a **StartOS updates** badge. Click **Add registry** and enter a registry's URL to add it; the keys it offers that your server does not already have become trusted for packages, and StartOS lists them. A registry's menu offers:

- **Manage keys** opens [Trusted Keys](trusted-keys.md) filtered to the keys the registry offers.
- **Use for StartOS updates** makes the registry the source of StartOS updates and trusts its keys for StartOS updates. StartOS first checks that the registry publishes StartOS updates, and warns before using any registry other than the Start9 Registry.
- **Remove** removes the registry. Afterward, StartOS offers to remove the keys it added that no connected registry still offers. The registry that provides StartOS updates cannot be removed until you choose another. **Remove** isn't offered for the Start9 Registry and the Community Registry; disconnect either with [`start-cli server registries remove`](cli-reference.md#start-cli-server-registries-remove-url).

## Package Signers

Package details include a **Signed by** section listing every key that signed the package: a green check for a key you trust for packages, a red X for a key you know but do not trust, and a gray dash with the key's fingerprint for a key your server does not know. Select a key to see its details and trust or distrust it for packages or StartOS updates; an unknown key needs a label first. Confirm a key's fingerprint through a source you already trust before trusting it. When no trusted signer matches, a warning appears, and installing asks you to confirm the risks first. Manage individual keys and their package and StartOS-update trust under **System → [Trusted Keys](trusted-keys.md)**.

## Download Transports

BitTorrent downloads are optional and disabled by default. When enabled and a registry advertises a BitTorrent source, StartOS prefers it for package downloads and OS updates, with HTTP fallback if the torrent is unavailable, stalls, or fails verification. Both transports use the same signed content commitments. BitTorrent sources must contain a single asset file. V1/v2 hybrid torrents use their v1-compatible swarm. Recovery-mode OS updates stream over HTTP directly to the image partition.

BitTorrent downloads buffer a complete asset on the data disk before unpacking package files or applying an OS update. Install progress appears once signed package metadata is ready, and the remaining transfer can be cancelled. Metadata can come from advertised HTTP mirrors or BitTorrent peers. Temporary transfer files are cleaned up afterward.

> [!WARNING]
> HTTP downloads already expose an IP address to the server. BitTorrent widens that audience to trackers and other peers, which can also see the requested torrent. Outgoing BitTorrent connections use the default gateway for StartOS, shown on **System → Software sharing**. When StartTunnel provides that gateway, outgoing peers see its exit IP, not the underlying connection’s public IP. Incoming connections expose whichever addresses are enabled below the controls. Onion registries use HTTP through Tor instead of BitTorrent.

Enable **Download using BitTorrent** under **System → Software sharing** to opt in, or run `start-cli server bittorrent set-preferences --downloads true`. Disabling it applies without restarting, stops active peer transfers and lets downloads fall back to HTTP.

### Software Sharing

Sharing compatible installed S9PKs and OS images already stored by StartOS is disabled by default. Enable **Share S9PKs and OS images** under **System → Software sharing**, or use:

```sh
start-cli server bittorrent set-preferences --downloads true --seeding true
```

Downloads and sharing stored software both start disabled. Changes apply live and persist across restarts. Active BitTorrent downloads can upload even when sharing stored software is off. Use `--seeding false` to stop sharing stored software, or `--downloads false` to stop all peer transfers.

StartOS discovers advertisements for installed packages from their recorded registry and for stored OS images from the selected update registry. With BitTorrent downloads enabled, it also remembers torrent metadata from new installations and updates, including downloads that fall back to HTTP. Files without an advertised source are not assigned a swarm.

Sharing requires an exact byte match with the advertised torrent. Installation can filter package architectures and remove supplemental signature trailers, leaving an installed S9PK that cannot join the original swarm. StartOS skips incompatible files without modifying them.

Sharing adds only source-path, commitment and torrent-metadata records under `/media/startos/data/package-data/registry-torrents`. It creates no extra payload copies or extended retention: installation, uninstall and native image cleanup still control the software files. Disabling sharing keeps this ordinary file management unchanged.

Uploads can use outgoing peer connections without an incoming port forward. To accept incoming peers, select LAN/private or public addresses in the **Incoming peer addresses** table on the same page. These address groups start off and require both downloads and sharing to be enabled. The first address opt-in allocates an incoming port. Before that, the table previews candidates without reserving a port; domain and URL actions become available after activation. Disabling every address or either switch releases the binding and its port. Re-enabling a switch requires a fresh address opt-in. Internal listen endpoints are not offered.

StartOS manages the TCP listener, external port allocation and forwarding through its existing interface system. Open an address’s standard **Address Requirements** dialog to check reachability. Native forwarding uses PCP/UPnP where supported; a configured listener or mapping alone does not establish public reachability.

The `directory` option under `bittorrent-seed` in `/media/startos/config/config.yaml` overrides the metadata-catalog location, not a payload-storage location. It does not enable sharing. The administrator’s `bittorrent: false` configuration disables peer transport and clears incoming address selections without changing the download/sharing preferences; changing that configuration requires a restart.

## Open Ecosystem

Anyone, anywhere can establish their own registry. This open model means no single entity controls which services are available, and no service can be effectively censored. Users choose which signing keys to trust independently of the hosting registry, and services can be distributed through multiple independent registries, direct file transfer, or built from source.
