# Clearnet

Make your services publicly reachable on the Internet using standard domains (`.com`, `.net`, etc.). This requires [gateway](gateways.md) selection, a domain name, DNS configuration, and port forwarding.

## WATCH THE VIDEO

<div class="yt-video" data-id="g3IKJ-6ysxA" data-title="Clearnet"></div>

> [!WARNING]
> If your ISP uses [CGNAT](cgnat.md), your router cannot accept inbound connections and port forwarding will not work. You **must** use a [StartTunnel](/start-tunnel/) gateway for clearnet hosting.

## Home Router vs Virtual Private Router

When hosting services on the clearnet, anyone who connects will know the IP address of the gateway used. Knowing a gateway's IP address reveals its approximate geographic location:

| Geographic Location     | Detection Accuracy    |
| ----------------------- | --------------------- |
| Country                 | 99%                   |
| State / Region          | 95-99%                |
| City (large metro)      | 60–80%                |
| Zip Code / Neighborhood | 30–50%                |
| Exact Street Address    | Requires ISP subpoena |

If your gateway is your home router, you are revealing the approximate location of your home. If your gateway is a virtual private router ([StartTunnel](/start-tunnel/)), you are revealing the approximate location of the VPS, not your home.

|                      | Router                                                                                                                                                                                                               | [StartTunnel](/start-tunnel/)                                   |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| **Cost**             | Free                                                                                                                                                                                                                 | VPS rental (~$5–10/mo)                                          |
| **IP stability**     | Home IP can change without warning, breaking all your domains until DNS is updated. [Dynamic DNS](https://en.wikipedia.org/wiki/Dynamic_DNS) is highly recommended, but support varies by router and may cost money. | Static IP from the VPS provider. No dynamic DNS needed.         |
| **Privacy**          | Exposes your home's approximate location                                                                                                                                                                             | Exposes the VPS location, not your home                         |
| **CGNAT compatible** | No. If your ISP uses [CGNAT](cgnat.md), you cannot use your router as a gateway.                                                                                                                                     | Yes                                                             |
| **Port forwarding**  | Configured in router admin panel                                                                                                                                                                                     | Configured in [StartTunnel](/start-tunnel/published-ports.html) |

## Add a Public Domain

1. On the service interface page, locate your preferred gateway and click "Add Domain", then select "Public Domain".

1. Enter the fully qualified domain name. For example, if you control `domain.com`, you could enter `domain.com` or `public.domain.com` or `nextcloud.public.domain.com`, etc.

1. Select a Certificate Authority to sign the certificate for this domain.
   - **Let's Encrypt**: Ideal for public access. All devices trust Let's Encrypt certificates by default. The domain serves that certificate and nothing else — while StartOS has yet to obtain one, the domain does not answer. Getting one requires port `443`, whatever port the interface itself uses; see [Configure Port Forwarding](#configure-port-forwarding).
   - **Local Root CA**: Ok for personal access. Bad for public access. Only devices that have downloaded and trusted your server's Root CA will be able to access the domain without issue.

1. Click "Save".

1. StartOS automatically tests your DNS record and port forwarding — plus the IPv6 firewall when your gateway has a GUA, and port `443` for a Let's Encrypt domain served on any other port. If everything passes, the domain is ready to use. Otherwise a setup modal appears showing what still needs attention, with instructions and the ability to re-test each check.

   > [!NOTE]
   > The port `443` test is a check, not a step. StartOS requests a Let's Encrypt certificate the first time an HTTPS connection for the domain reaches it, and a failed test does not stop that. The authority validates on port `443` regardless of which port the address itself uses, and until the first certificate is issued the address answers nothing.
   >
   > To run the test, StartOS asks an outside echo service (`ipconfig.io`, then `ifconfig.co`) to connect back to the gateway's public IP on that port. The request leaves through that gateway, so over StartTunnel it comes from the VPS's IP, not your home connection. If StartOS has already opened the port automatically, it skips the echo service and reports the mapping instead.
   >
   > The test applies while the domain is waiting on that certificate and again through each renewal window — a domain being renewed keeps serving the certificate it holds, so keep `443` forwarded and it renews without interruption. See [Configure Port Forwarding](#configure-port-forwarding); the rule also appears under **System → Gateways → Port Forwards**.

## Set Up DNS Records

StartOS tests DNS automatically when you add or enable a public domain, and will guide you through the setup if the test fails. For reference, here is what is needed:

1. Access your domain's DNS settings, usually in the registrar where you originally leased the domain.

1. Create a DNS record that points your domain to your gateway's public IP address:
   - An **`A` record** pointing at your gateway's public **IPv4** address.
   - If your gateway also has an IPv6 **global-unicast address (GUA)**, add an **`AAAA` record** pointing at that address. StartOS serves a public domain over both IPv4 and IPv6 (DualStack) whenever a GUA is available, so adding both records lets clients reach the domain over either protocol. The exact record values are shown in the DNS setup modal.

   If you use subdomains, consider using a wildcard (`*`) for that host so that all future subdomains work without needing additional records.

   > [!WARNING]
   > For a Let's Encrypt domain, set the record to **DNS only**. A record proxied through Cloudflare (the orange cloud) points the name at Cloudflare instead of your gateway, so the DNS test fails and Let's Encrypt cannot validate the domain.

   > [!TIP]
   > It can take up to a few hours for DNS changes to propagate. You can check propagation using [https://dnschecker.org](https://dnschecker.org).

## Configure Port Forwarding

To expose a public domain to the Internet, the appropriate port must be forwarded in the corresponding gateway. StartOS tests port forwarding automatically when you add or enable a public domain, and will guide you through the setup if the test fails.

When a public address is enabled, StartOS first **attempts to open the port automatically** on the corresponding gateway, using PCP (preferred), then NAT-PMP, then UPnP. If the gateway supports one of these (and it is enabled), no manual step is required — and when the address is later disabled or deleted, StartOS removes the port forward it created. This is best-effort: if the gateway supports none of them, the automatic test will fail and you create the rule manually as described below.

> [!NOTE]
> IPv6 needs no port _forwarding_. An IPv6 GUA is your server's own globally-routable address, so there is nothing to translate — StartOS instead opens an inbound **firewall pinhole** for the domain's port on the upstream gateway (again via PCP) and serves the domain on the GUA directly. The bare GUA itself stays local-only: only traffic that matches the domain (by TLS SNI) is accepted, so exposing a public domain over IPv6 does not expose the address for anything else.

> [!TIP]
> Most websites and APIs on the Internet are hosted on port `443`. Port `443` is so common, in fact, that apps and browsers _infer_ its presence. The _absence_ of a port _means_ the port is `443`. With rare exceptions, domains on StartOS also use port `443`, and that is why your domains usually do not display a port. The port forwarding rule needed for these standard domains is always the same, which means you only have to do it once!

> [!IMPORTANT]
> A Let's Encrypt domain needs port `443` even when its interface is served somewhere else — an Electrum server on `50002`, for example. Let's Encrypt proves you control a name by connecting to it on port `443`, regardless of which port the service behind it uses, and StartOS answers there for every domain it holds an authority for. Over StartTunnel, StartOS claims `443` for that domain automatically alongside the interface's own port, and over IPv6 it opens the matching `443` pinhole, so there is nothing to do. To confirm it, open **Published Ports** in StartTunnel: the **Automatic** table should list port `443` to your server with the exact domain as its hostname. If it is missing, check that auto-publish is on for the server, or add the entry by hand — External `443`, your server, Internal `443`, IPv4, and the domain as its [Hostname](/start-tunnel/published-ports.html#sni-hostnames-ipv4-only). On a router, forward `443` to your server as you would for a standard domain, and keep it forwarded — the certificate is renewed the same way, and the rule is listed for you under **System → Gateways → Port Forwards**. Such a domain is served on its own port only, so reach it as `https://<name>:<port>`. Choose **Local Root CA** instead for a domain whose gateway cannot offer port `443`.

How you create a port forwarding rule depends on the type of gateway.

- **Routers**: Port forwarding is supported by all routers and easy to do. Many routers also support PCP, NAT-PMP, or UPnP, in which case StartOS opens the port for you automatically. If none is available or they are disabled, refer to your router's manual to add the rule manually.

- **StartTunnel**: StartTunnel supports PCP and UPnP over the tunnel, so StartOS opens the required port automatically. To add or manage forwards manually, refer to the [StartTunnel Published Ports guide](/start-tunnel/published-ports.html).

## Troubleshooting Let's Encrypt

- **`Network Error … error sending request for url (https://ifconfig.co/port/443)`.** StartOS could not reach the echo service it uses for the port test. That says nothing about whether the port is open, and it does not block the certificate. Only the last service's error is shown, so `ipconfig.io` failed too. Over StartTunnel, compare by running `curl -v https://ifconfig.co/port/443` on the VPS; running it on your server tests your home connection instead. Because StartOS skips the echo service when it holds an automatic mapping for the port, this error over StartTunnel also means that mapping is missing: check **Published Ports** as described in [Configure Port Forwarding](#configure-port-forwarding).

- **No certificate, no "Could not get a certificate" notification, and no ACME lines in the logs** (such as `performing ACME auth challenge` or `ACME order failed`). No HTTPS connection for the domain has reached your server yet, so StartOS has had no reason to request one. Check that the DNS record points at your gateway and is not proxied, and that port `443` reaches your server for that domain: on StartTunnel, a `443` entry with the domain as its hostname; on a router, a `443` forward.

- **A "Could not get a certificate for …" notification.** The request reached Let's Encrypt and failed. The notification includes the authority's reason.
