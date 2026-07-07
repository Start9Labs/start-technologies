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

| | Router | [StartTunnel](/start-tunnel/) |
| --- | --- | --- |
| **Cost** | Free | VPS rental (~$5–10/mo) |
| **IP stability** | Home IP can change without warning, breaking all your domains until DNS is updated. [Dynamic DNS](https://en.wikipedia.org/wiki/Dynamic_DNS) is highly recommended, but support varies by router and may cost money. | Static IP from the VPS provider. No dynamic DNS needed. |
| **Privacy** | Exposes your home's approximate location | Exposes the VPS location, not your home |
| **CGNAT compatible** | No. If your ISP uses [CGNAT](cgnat.md), you cannot use your router as a gateway. | Yes |
| **Port forwarding** | Configured in router admin panel | Configured in [StartTunnel](/start-tunnel/port-forwarding.html) |

## Add a Public Domain

1. On the service interface page, locate your preferred gateway and click "Add Domain", then select "Public Domain".

1. Enter the fully qualified domain name. For example, if you control `domain.com`, you could enter `domain.com` or `public.domain.com` or `nextcloud.public.domain.com`, etc.

1. Select a Certificate Authority to sign the certificate for this domain.
   - **Let's Encrypt**: Ideal for public access. All devices trust Let's Encrypt certificates by default.
   - **Local Root CA**: Ok for personal access. Bad for public access. Only devices that have downloaded and trusted your server's Root CA will be able to access the domain without issue.

1. Click "Save".

1. StartOS will automatically test your DNS record and port forwarding. If both pass, the domain is ready to use. If either test fails, a setup modal will appear showing the failing tests with instructions to remedy and the ability to re-test.

## Set Up DNS Records

StartOS tests DNS automatically when you add or enable a public domain, and will guide you through the setup if the test fails. For reference, here is what is needed:

1. Access your domain's DNS settings, usually in the registrar where you originally leased the domain.

1. Create a DNS record that points your domain to your gateway's public IP address:

   - An **`A` record** pointing at your gateway's public **IPv4** address.
   - If your gateway also has an IPv6 **global-unicast address (GUA)**, add an **`AAAA` record** pointing at that address. StartOS serves a public domain over both IPv4 and IPv6 (DualStack) whenever a GUA is available, so adding both records lets clients reach the domain over either protocol. The exact record values are shown in the DNS setup modal.

   If you use subdomains, consider using a wildcard (`*`) for that host so that all future subdomains work without needing additional records.

   > [!TIP]
   > It can take up to a few hours for DNS changes to propagate. You can check propagation using [https://dnschecker.org](https://dnschecker.org).

## Configure Port Forwarding

To expose a public domain to the Internet, the appropriate port must be forwarded in the corresponding gateway. StartOS tests port forwarding automatically when you add or enable a public domain, and will guide you through the setup if the test fails.

When a public address is enabled, StartOS first **attempts to open the port automatically** on the corresponding gateway, using PCP (preferred), then NAT-PMP, then UPnP. If the gateway supports one of these (and it is enabled), no manual step is required — and when the address is later disabled or deleted, StartOS removes the port forward it created. This is best-effort: if the gateway supports none of them, the automatic test will fail and you create the rule manually as described below.

> [!NOTE]
> IPv6 needs no port _forwarding_. An IPv6 GUA is your server's own globally-routable address, so there is nothing to translate — StartOS instead opens an inbound **firewall pinhole** for the domain's port on the upstream gateway (again via PCP) and serves the domain on the GUA directly. The bare GUA itself stays local-only: only traffic that matches the domain (by TLS SNI) is accepted, so exposing a public domain over IPv6 does not expose the address for anything else.

> [!TIP]
> Most websites and APIs on the Internet are hosted on port `443`. Port `443` is so common, in fact, that apps and browsers _infer_ its presence. The _absence_ of a port _means_ the port is `443`. With rare exceptions, domains on StartOS also use port `443`, and that is why your domains usually do not display a port. The port forwarding rule needed for these standard domains is always the same, which means you only have to do it once!

How you create a port forwarding rule depends on the type of gateway.

- **Routers**: Port forwarding is supported by all routers and easy to do. Many routers also support PCP, NAT-PMP, or UPnP, in which case StartOS opens the port for you automatically. If none is available or they are disabled, refer to your router's manual to add the rule manually.

- **StartTunnel**: StartTunnel supports PCP and UPnP over the tunnel, so StartOS opens the required port automatically. To add or manage forwards manually, refer to the [StartTunnel Port Forwarding guide](/start-tunnel/port-forwarding.html).
