# Interfaces

A service interface is a network endpoint exposed by a service running on your server. Every installed service exposes one or more interfaces, each serving a different purpose. The service interfaces for a given service are listed under the **Interfaces** tab in the sidebar of its service details page.

## Interface Types

- **UI** — A web-based user interface for interacting with the service in a browser. Examples: Vaultwarden web vault, Nextcloud dashboard, StartOS admin UI.

- **API** — A programmatic endpoint for apps and tools to communicate with the service. Examples: Bitcoin RPC, LND gRPC, Nextcloud WebDAV.

- **P2P** — A peer-to-peer endpoint for the service to communicate with other nodes on its network. Examples: Bitcoin P2P, Lightning P2P.

## Opening a UI

The **Open UI** button on a running service opens its web interface, and offers a menu of them when the service has more than one. StartOS picks which address to open from the way you are reaching StartOS itself — a `.local` name for a `.local` session, an onion address for a Tor session, a domain from outside your network — so it usually lands on an address your browser can resolve. A service reachable only on your local network has nothing else to offer a browser that is away from it, and you will get a local address that does not load.

A few services work at only one address, the one they were configured with, and those tell StartOS which it is. **Open UI** then opens that address and keeps opening it, because for such a service any other address rejects you. So if it opens something you cannot reach from where you are, the address to change is the service's own, through whichever action it gives you for choosing a primary URL — that action is also how you point it somewhere else. Switching that address off in the tables below has the same effect, since StartOS only opens an address you have enabled.

Which browser you are in still limits what a service can ask for. An onion address is opened from a Tor session and from no other, and from a Tor session StartOS honors only an onion or a public address, never one on your local network — Tor Browser cannot reach your LAN. Where a service's choice is ruled out this way, **Open UI** goes back to choosing for you.

To open one particular address instead, open it from its own row in the gateway tables below. An onion address has no such control — copy its URL and paste it into Tor Browser.

## Viewing Interface Addresses

Open the **Interfaces** tab to see every interface the service exposes. Each interface expands to reveal its **addresses** — all the ways that interface can be reached, organized by [gateway](gateways.md). When a service exposes only one interface, it is expanded by default.

### Gateway Tables

Each inbound gateway on your server has its own table. The rows in each table are the addresses available through that gateway. Addresses can be individually enabled or disabled.

Each table has the following columns:

| Column                    | Description                                                                                                                                                                                                                                                                                                                                           |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Toggle**                | Enable or disable the address. This directly affects iptables and firewall rules — disabling an address blocks traffic to it. Public IPv4 addresses are off by default. All other addresses are on by default.                                                                                                                                        |
| **Access**                | **Public** or **Private**. Public addresses are reachable from the Internet. Private addresses are only reachable on the LAN or via VPN. **For an IPv6 global-unicast (GUA) address this column is an editable Local / Public dropdown** — see the note below.                                                                                        |
| **Type**                  | The address type: `IPv4`, `IPv6`, `Domain`, or `mDNS` (mDNS is only available on router gateways).                                                                                                                                                                                                                                                    |
| **Certificate Authority** | Who signs the SSL certificate for this address: **Root CA** (your server's own CA), **Let's Encrypt** (publicly trusted), or **None** (non-SSL, e.g. plain HTTP).                                                                                                                                                                                     |
| **URL**                   | The fully composed URL for reaching the interface at this address.                                                                                                                                                                                                                                                                                    |
| **Actions**               | Context-dependent buttons: **Settings** (view and test the address's external requirements — DNS records, port forwarding, and, for a DualStack domain, the IPv6 firewall), **Delete** (remove domains that were manually added), **Open** (open the URL in a new tab), **Copy** (copy the URL to clipboard), **QR** (display a QR code for the URL). |

> [!NOTE]
> The Settings button appears for addresses that require external configuration: [public domains](clearnet.md) (DNS + port forwarding), [private domains](private-domains.md) (DNS), and [public IP addresses](public-ip.md) (port forwarding).

> [!NOTE]
> An address whose Certificate Authority is **Let's Encrypt** serves a Let's Encrypt certificate and nothing else. While StartOS has yet to obtain one — the DNS record has not propagated yet, or port `443` does not reach your server for that domain — the address does not answer, rather than presenting your server's Root CA under a name you asked a public authority to sign. See [Configure Port Forwarding](clearnet.md#configure-port-forwarding), which covers what a domain on a port other than `443` needs.

> [!NOTE]
> The port-forwarding and firewall tests need the service **running** only for an address it serves directly — a raw public IP, or another non-SSL binding. StartOS SSL-terminates every HTTP interface behind its always-on reverse proxy, so those stay testable even while the service is stopped (as do DNS tests). A non-SSL address's Test buttons are therefore disabled while its service is stopped; and because that service often restarts when a domain is added or an address is enabled, StartOS then shows its reachability tests as untested (not failed) and still opens the setup modal, so you can set up forwarding and re-test once it is running.

> [!NOTE]
> For an interface whose TLS StartOS terminates, StartOS allows up to 15 seconds to connect to the service and complete any required TLS handshake with it. If that connection is not ready in time, the client connection ends.

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

A domain you add belongs to the interface you added it to. If an update reorganizes a service's interfaces and retires the one your domain was on, the domain is removed with it — check the service's release notes, which should say when an update does this, and add the domain to one of the service's current interfaces.

### Tor Onion Addresses

If the [Tor](tor.md) service is installed and running, a **Tor** table also appears among the interface's addresses. Tor functions like a gateway but is managed as a marketplace service rather than a system gateway.

The Tor table is empty by default. To add onion addresses:

1. Open the Tor service and go to **Actions > Manage Onion Services**.

1. Select the service interface you want to create an onion address for.

1. Each onion address produces both an `HTTP` and `HTTPS` URL. The `HTTP` address is perfectly safe to use because Tor is a secure protocol. The `HTTPS` address uses a certificate signed by your server's Root CA.

1. Optionally, you can upload a private key to use a [vanity `.onion` address](https://community.torproject.org/onion-services/advanced/vanity-addresses/).

1. To view your onion addresses, go to **Actions > View Onion Addresses**. They will also appear in the Tor table for each service interface.
