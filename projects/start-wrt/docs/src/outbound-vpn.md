# Outbound VPNs

Route your network's Internet traffic through one or more WireGuard VPN providers for privacy. Outbound VPNs hide your home IP address from the services your devices connect to and prevent your ISP from monitoring traffic.

## Adding a VPN Client

1. Navigate to `Internet > Outbound VPNs` and click "Add".

1. Configure the VPN:
   - **Label** — A descriptive name of up to 12 characters (e.g. "Mullvad SE", "Proton US").
   - **Config File** — Upload a WireGuard `.conf` file from your VPN provider — drop it into the dialog or click to browse. Most providers (Mullvad, ProtonVPN, IVPN, etc.) offer WireGuard config file downloads from their account dashboard.
   - **Connects via** — How this VPN reaches its own VPN server. The dropdown lists **Internet** along with your existing VPNs. Select **Internet** to connect over your WAN, or select another VPN to carry this one's tunnel, so traffic passes through that VPN first for additional privacy.
   - **DNS Servers** — Optional. Replaces the DNS servers in the config file's `DNS =` line. See [DNS](#dns).

1. Click "Add VPN".

> [!TIP]
> Download a config file for a VPN server location near you for best performance.

## VPN Chaining

VPN chaining routes traffic through multiple VPN providers in sequence, so no single provider sees both your identity and your destination. This achieves multi-jurisdictional resilience — the providers would need to collaborate across different legal jurisdictions to correlate your activity.

Chaining is configured through the **Connects via** field. The VPN that a VPN connects via carries its tunnel, so that VPN is the hop nearer your WAN and the VPN itself is the hop nearer the Internet. Traffic leaves your network wrapped in both tunnels and is unwrapped one server at a time:

```
Your device → StartWRT → VPN that it connects via → This VPN → Internet
```

For example, if "Mullvad" connects via "Proton" and "Proton" connects via "Internet", a profile routed through Mullvad reaches Proton's server first and exits at Mullvad's:

- Proton knows your home IP, and sees only that you are connecting to Mullvad — never your destination.
- Mullvad knows your destination, and sees Proton's IP as the source — never yours.
- The site you visit sees Mullvad's IP.

Neither provider holds both halves. To link you to your destination they would have to compare logs with each other, across whatever jurisdictions they operate in.

A VPN can connect through another only when:

- The tunnel of the VPN that it connects via can carry it: that VPN has an address of the same family (IPv4 or IPv6) and its `AllowedIPs` include this VPN's server address.
- No other VPN uses the same server address.

If a VPN's config names its server by hostname, the router looks the hostname up through the tunnel of the VPN it connects via, so the lookup never leaves through your WAN. It asks the DNS servers that profiles routed through that VPN use (see [DNS](#dns)), in plain DNS even when they are set to DNS-over-HTTPS. It uses the server's IPv4 address when that VPN can carry one, and looks the hostname up again if the server stops answering.

If a VPN goes down, every VPN that connects via it stops rather than reconnecting to its server over your WAN, so your home IP is never shown to the provider you meant to hide it from.

Each tunnel adds its own headers, so every VPN gets the largest MTU that fits through what it connects via: the MTU of that VPN, or of your WAN connection (1500, or 1492 for PPPoE), less 60 bytes for an IPv4 server or 80 for an IPv6 one. A server connecting over your WAN that is named by hostname counts as IPv6 while your WAN has IPv6. A VPN with an IPv4 server gets 1440 over the WAN, and 1380 via another VPN at 1440. The router recalculates these whenever a VPN or your WAN connection changes. It refuses a change that would leave a VPN an MTU below 1280: three VPNs chained through IPv6 servers need an IPv4 server for at least one of the two that connect via another.

> [!NOTE]
> VPN chaining adds latency since traffic passes through multiple servers. For most users, a single VPN provider is sufficient.

## IPv6 and Kill Switch

How IPv6 traffic is handled depends on whether the VPN's WireGuard tunnel supports it:

- **IPv6-capable VPN** — If the imported config includes an IPv6 address for the tunnel interface, profiles routed through the VPN send their IPv6 traffic (`::/0`) through the tunnel, just like IPv4.
- **IPv4-only VPN** — If the tunnel has no IPv6 address, no IPv6 flows through it: IPv6 internet traffic is blocked (it "fails closed") so it cannot leak around the tunnel and out your WAN. Non-admin profiles routed through the VPN get no IPv6 addressing at all; the Admin profile's [LAN IPv6](lan.md) setting stays under your control, so its devices still receive IPv6 addresses — just without IPv6 internet.

A kill switch protects every VPN-routed profile: both IPv4 and IPv6 fail closed. If the tunnel goes down, traffic is blocked rather than leaking out the WAN.

> [!NOTE]
> Routing IPv6 through a VPN requires the VPN's tunnel config to include an IPv6 address on the tunnel interface. Without one, the VPN carries IPv4 only and IPv6 is blocked for routed profiles.

## DNS

A profile routed through a VPN sends its DNS queries through the tunnel, using the first of these that is set:

1. The profile's own custom DNS servers, including DNS-over-HTTPS servers.
1. The VPN's **DNS Servers** — from the config file's `DNS =` line, or set on the VPN's detail page.
1. The custom servers in [WAN Settings](wan.md#dns).
1. `1.1.1.1` (Cloudflare). The profile's DNS column then shows **Fallback (1.1.1.1)**.

Like the rest of the profile's traffic, these queries fail closed: if the tunnel goes down, DNS stops rather than falling back to your WAN. They never go to your ISP's DNS servers.

## VPN Detail Page

Click a VPN label in the table to open its detail page, which shows:

- **Status** — The tunnel's live state, with a note explaining it:
  - **Connected** — The tunnel completed a handshake within the last three minutes.
  - **Idle** — Enabled, but nothing has sent traffic through the tunnel recently, so it has no active session. It connects when a profile using it sends traffic.
  - **Not responding** — The router is sending to the VPN server and getting no reply. Profiles using this VPN have no internet until it responds. Check with your provider that the server is online and that the configuration is still valid.
  - **Disabled** — The VPN is turned off.
- **Last handshake** — How long ago the tunnel last completed a handshake with the VPN server, or **Never**. Shown while the VPN is enabled.
- **Connection Path** — The encrypted tunnels this VPN's traffic travels through, drawn as pipes. Each tunnel starts at the router and ends at its VPN server, and a VPN that connects via another runs its tunnel inside that VPN's. Traffic routed here leaves for the Internet from this VPN's server.
- **Used by** — Which [Security Profiles](security-profiles.md) currently route their traffic through this VPN. Check this before making changes to understand the impact.
- **Label** — Edit the display name.
- **Connects via** — Change what this VPN connects via (Internet or another VPN). Only choices that would not create a circular chain are offered.
- **MTU** — The tunnel's packet size limit. Leave it blank to use the largest MTU that fits, shown in the field and described under [VPN Chaining](#vpn-chaining). Lower it — down to a minimum of 1280 — only if the VPN connects but sites won't load. An MTU in the imported config file is kept only when it is smaller than what fits.

To delete a VPN, click "Delete" on its detail page. If any Security Profiles route through the VPN, you will be asked to confirm — those profiles will switch to the regular WAN connection.

> [!NOTE]
> You cannot delete a VPN while other VPNs connect via it. Change their **Connects via** first.

## Enabling and Disabling

Each VPN has an enable/disable toggle in the table view. Disabling a VPN switches every profile that routes through it to the WAN (direct Internet), after you confirm. Those profiles stay on the WAN until you select the VPN again under their **Outbound Routing**. A disabled VPN is not offered when choosing a [Security Profile's](security-profiles.md) outbound routing, nor under another VPN's **Connects via** — re-enable it first.

> [!NOTE]
> You cannot disable a VPN while other VPNs connect via it. Change their **Connects via** first.

## Assigning VPNs to Profiles

By default, all [Security Profiles](security-profiles.md) use the router's default gateway (your ISP) for Internet traffic. You can override this per profile:

1. Navigate to `Security Profiles` and select "Edit" from the profile's actions menu.

1. Under **Outbound Routing**, select a VPN client.

1. Click "Save".

This lets you route different profiles through different VPNs. For example:

- **Admin** profile routes through Mullvad
- **Children** profile routes through a DNS-filtering VPN
- **Guest** profile routes through Proton
- **Smart Devices** profile uses the default gateway (no VPN)
