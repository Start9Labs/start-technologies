# Interfaces

A service interface is a network endpoint exposed by a service running on your server. Every installed service exposes one or more interfaces, each serving a different purpose. The service interfaces for a given service are listed under the **Interfaces** tab in the sidebar of its service details page.

## Interface Types

- **UI** — A web-based user interface for interacting with the service in a browser. Examples: Vaultwarden web vault, Nextcloud dashboard, StartOS admin UI.

- **API** — A programmatic endpoint for apps and tools to communicate with the service. Examples: Bitcoin RPC, LND gRPC, Nextcloud WebDAV.

- **P2P** — A peer-to-peer endpoint for the service to communicate with other nodes on its network. Examples: Bitcoin P2P, Lightning P2P.

## Viewing Interface Addresses

Open the **Interfaces** tab to see every interface the service exposes. Each interface expands to reveal its **addresses** — all the ways that interface can be reached, organized by [gateway](gateways.md). When a service exposes only one interface, it is expanded by default.

### Gateway Tables

Each inbound gateway on your server has its own table. The rows in each table are the addresses available through that gateway. Addresses can be individually enabled or disabled.

Each table has the following columns:

| Column                    | Description                                                                                                                                                                                                                                                                                                                    |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Toggle**                | Enable or disable the address. This directly affects iptables and firewall rules — disabling an address blocks traffic to it. Public IPv4 addresses are off by default. All other addresses are on by default.                                                                                                                 |
| **Access**                | **Public** or **Private**. Public addresses are reachable from the Internet. Private addresses are only reachable on the LAN or via VPN. **For an IPv6 global-unicast (GUA) address this column is an editable Local / Public dropdown** — see the note below.                                                                 |
| **Type**                  | The address type: `IPv4`, `IPv6`, `Domain`, or `mDNS` (mDNS is only available on router gateways).                                                                                                                                                                                                                             |
| **Certificate Authority** | Who signs the SSL certificate for this address: **Root CA** (your server's own CA), **Let's Encrypt** (publicly trusted), or **None** (non-SSL, e.g. plain HTTP).                                                                                                                                                              |
| **URL**                   | The fully composed URL for reaching the interface at this address.                                                                                                                                                                                                                                                             |
| **Actions**               | Context-dependent buttons: **Settings** (view required DNS records, DNS configuration, and/or port forwarding rules, with the ability to test each), **Delete** (remove domains that were manually added), **Open** (open the URL in a new tab), **Copy** (copy the URL to clipboard), **QR** (display a QR code for the URL). |

> [!NOTE]
> The Settings button appears for addresses that require external configuration: [public domains](clearnet.md) (DNS + port forwarding), [private domains](private-domains.md) (DNS), and [public IP addresses](public-ip.md) (port forwarding).

> [!NOTE]
> Unlike a private LAN address, an IPv6 **global-unicast address (GUA)** is a single globally-routable address, so how far it reaches is a choice. A GUA row keeps the usual on/off toggle, and its **Access** column becomes a **Local / Public** dropdown:
>
> - **Local** (default) — reachable on the local network only; traffic from outside your subnet is rejected.
> - **Public** — also reachable from the Internet. StartOS attempts to open the matching pinhole on your gateway automatically (via PCP); if your gateway doesn't support it you may need to allow inbound traffic to that address and port manually.
>
> This only applies to IPv6 GUAs. IPv6 ULAs (private) are always local, and IPv4 keeps its separate LAN and WAN address rows.

### Adding Domains

You can add domains to a gateway table by clicking "Add Domain" on the gateway and choosing either:

- **[Public Domain](clearnet.md)** — A clearnet domain (e.g. `mysite.com`) accessible from the Internet. Requires DNS configuration and port forwarding.
- **[Private Domain](private-domains.md)** — A custom domain (e.g. `nextcloud.private`) that works on LAN and VPN. Requires your gateway to use StartOS for DNS. Only available on Ethernet and Wireless gateways — not WireGuard (StartTunnel) gateways.

### Tor Onion Addresses

If the [Tor](tor.md) service is installed and running, a **Tor** table also appears among the interface's addresses. Tor functions like a gateway but is managed as a marketplace service rather than a system gateway.

The Tor table is empty by default. To add onion addresses:

1. Open the Tor service and go to **Actions > Manage Onion Services**.

1. Select the service interface you want to create an onion address for.

1. Each onion address produces both an `HTTP` and `HTTPS` URL. The `HTTP` address is perfectly safe to use because Tor is a secure protocol. The `HTTPS` address uses a certificate signed by your server's Root CA.

1. Optionally, you can upload a private key to use a [vanity `.onion` address](https://community.torproject.org/onion-services/advanced/vanity-addresses/).

1. To view your onion addresses, go to **Actions > View Onion Addresses**. They will also appear in the Tor table for each service interface.
