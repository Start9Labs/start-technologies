# Port Forwarding

Port forwarding exposes a device's port to the public Internet. StartTunnel can do this over **IPv4** (a DNAT from your VPS's public IPv4) and over **IPv6** (a firewall _pinhole_ on the device's own global address) — one dialog handles both.

> [!WARNING]
> **IPv4** forwarding requires a **dedicated public IPv4 address** on your VPS. Shared IPv4 addresses (CGNAT, shared NAT, load-balanced IPs) cannot be used to expose services to the clearnet. **IPv6** forwarding instead needs a routed IPv6 prefix delegated to the subnet (see [IPv6](./ipv6.md)); each device then has its own globally-routable address, so a dedicated public IPv4 isn't required for the IPv6 path.

> [!NOTE]
> StartTunnel acts as a port-control gateway for connected devices, speaking PCP (preferred) and UPnP. A StartOS server using this tunnel opens the ports it needs **automatically** — over IPv4 and, on a current StartOS (0.4.0-beta.10+) with an IPv6 prefix, over IPv6 too — when you enable a public address, and removes them when the address is disabled or deleted. Each automatic PCP mapping carries a **lease** that the device renews while it still wants the port; if the device stops renewing (it goes offline, or you withdraw the exposure), the tunnel drops the forward on its own rather than letting it linger. For security, an automatically created forward or pinhole always targets the requesting device's own address; a device can only open ports to itself. The steps below are for adding or managing forwards manually.

## IPv4 forwards vs. IPv6 pinholes

- An **IPv4** forward is a DNAT: clients connect to your VPS's public IPv4 on the external port, and the tunnel rewrites the destination to the device's tunnel IP and internal port.
- An **IPv6** forward is a _pinhole_: the device already has a globally-routable address (its GUA — see [IPv6](./ipv6.md)), so there is no NAT. The tunnel simply permits inbound to `[GUA]:port`. If you pick an external port different from the internal port (e.g. an `80 → 443` redirect) it becomes a port-only translation on that same address.

Because each device has its own IPv6 address, two different devices can both publish on the same external port over IPv6 (whereas over IPv4 they share one public address, so external ports must be unique).

## Manual and automatic forwards

The `Port Forwards` page shows two tables: **Manual** forwards you added by hand, and **Automatic** forwards opened by connected devices via PCP/UPnP. A row's **External IP** is your VPS's public IPv4 (a v4 forward) or the device's IPv6 GUA (a v6 pinhole). You can enable, disable, or remove either; automatic forwards have no editable label (they're owned by the device that created them) and may be re-created if you remove one while the device still wants it. Manual forwards are persistent — they stay until you delete them. Automatic forwards are lease-based: one that stops being renewed (its device went offline or no longer wants the port) expires and is removed on its own.

Deleting a device or demoting it to a client clears all of its forwards (manual and automatic, IPv4 and IPv6). Turning off **automatic port forwarding** for a device clears its automatic forwards but leaves any you added by hand.

## Add a forward manually

1. In StartTunnel, navigate to `Port Forwards` and click "Add".

1. Enter the **External Port**, select the **Server** (the device to forward to), and enter the **Internal Port**. In almost all cases they are the same.

1. Choose the **IP Version** — `IPv4`, `IPv6`, or `IPv4 + IPv6`. `IPv6` and `IPv4 + IPv6` require the selected server to have an IPv6 address, which means its subnet must carry a routed IPv6 prefix (see [IPv6](./ipv6.md)); the dialog tells you when the chosen server has none.

1. To forward a **range** of ports, set "Number of Ports" to the size of the range. It counts up from both the external and internal ports — e.g. external `49152`, internal `49152`, count `100` forwards `49152–49251` on each side. Leave it at `1` for a single port.

1. If you are forwarding port `443 -> 443`, you will see a checkbox to also forward port `80 -> 443`. This is highly recommended, as it auto-redirects HTTP to HTTPS. It applies to whichever IP versions you selected.

1. Click "Save".

## SNI hostnames (IPv4 only)

When IP Version includes IPv4 (`IPv4` or `IPv4 + IPv6`), an optional **Hostname** routes by TLS SNI so several hostnames can share one external port. SNI demultiplexing is IPv4-only — in `IPv4 + IPv6` mode it applies to the IPv4 side only, and the IPv6 side is a plain pinhole (each device already has its own address, so no demux is needed) — and it cannot be combined with a port range.
