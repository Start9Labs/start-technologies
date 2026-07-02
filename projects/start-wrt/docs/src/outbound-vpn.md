# Outbound VPNs

Route your network's Internet traffic through one or more WireGuard VPN providers for privacy. Outbound VPNs hide your home IP address from the services your devices connect to and prevent your ISP from monitoring traffic.

## Adding a VPN Client

1. Navigate to `Internet > Outbound VPNs` and click "Add".

1. Configure the VPN:

   - **Label** — A descriptive name (e.g. "Mullvad Sweden", "Proton US").
   - **Config File** — Upload or paste a WireGuard `.conf` file from your VPN provider. Most providers (Mullvad, ProtonVPN, IVPN, etc.) offer WireGuard config file downloads from their account dashboard.
   - **Target** — Where this VPN's traffic should be routed:
     - **Internet** — Traffic exits directly to the Internet through this VPN.
     - **Another VPN** — Chain through another VPN first for additional privacy. Select the target VPN from the dropdown. Only VPNs that would not create a circular chain are shown.

1. Click "Save".

> [!TIP]
> Download a config file for a VPN server location near you for best performance.

## VPN Chaining

VPN chaining routes traffic through multiple VPN providers in sequence, so no single provider sees both your identity and your destination. This achieves multi-jurisdictional resilience — the providers would need to collaborate across different legal jurisdictions to correlate your activity.

Chaining is configured through the **Target** field. When you set a VPN's target to another VPN instead of "Internet", traffic flows through both:

```
Your device → StartWRT → First VPN → Second VPN → Internet
```

For example, if "Mullvad" targets "Proton" and "Proton" targets "Internet":
- Mullvad knows your home IP but not your destination.
- Proton knows your destination but sees Mullvad's IP, not yours.

> [!NOTE]
> VPN chaining adds latency since traffic passes through multiple servers. For most users, a single VPN provider is sufficient.

## IPv6 and Kill Switch

How IPv6 traffic is handled depends on whether the VPN's WireGuard tunnel supports it:

- **IPv6-capable VPN** — If the imported config includes an IPv6 address for the tunnel interface, profiles routed through the VPN send their IPv6 traffic (`::/0`) through the tunnel, just like IPv4.
- **IPv4-only VPN** — If the tunnel has no IPv6 address, profiles routed through it get no IPv6. IPv6 is blocked (it "fails closed") so it cannot leak around the tunnel and out your WAN.

A kill switch protects every VPN-routed profile: both IPv4 and IPv6 fail closed. If the tunnel goes down, traffic is blocked rather than leaking out the WAN.

> [!NOTE]
> Routing IPv6 through a VPN requires the VPN's tunnel config to include an IPv6 address on the tunnel interface. Without one, the VPN carries IPv4 only and IPv6 is blocked for routed profiles.

## VPN Detail Page

Click a VPN label in the table to open its detail page, which shows:

- **Connection Path** — The full route traffic takes from this VPN to the Internet (e.g. "Mullvad → Proton → Internet").
- **Used by** — Which [Security Profiles](security-profiles.md) currently route their traffic through this VPN. Check this before making changes to understand the impact.
- **Label** — Edit the display name.
- **Connects to** — Change the target (Internet or another VPN).

To delete a VPN, click "Delete" on its detail page.

> [!NOTE]
> You cannot delete a VPN if other VPNs use it as a target. Change their target first.

## Enabling and Disabling

Each VPN has an enable/disable toggle in the table view. When a VPN is disabled, profiles that route through it will fall back to the WAN (direct Internet).

> [!NOTE]
> You cannot disable a VPN if other VPNs use it as a target. Change their target first.

## Assigning VPNs to Profiles

By default, all [Security Profiles](security-profiles.md) use the router's default gateway (your ISP) for Internet traffic. You can override this per profile:

1. Navigate to `Security Profiles` and select a profile.

1. Under **Outbound Routing**, select a VPN client.

1. Click "Save".

This lets you route different profiles through different VPNs. For example:

- **Admin** profile routes through Mullvad
- **Children** profile routes through a DNS-filtering VPN
- **Guest** profile routes through Proton
- **Smart Devices** profile uses the default gateway (no VPN)
