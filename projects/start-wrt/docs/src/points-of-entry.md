# Points of Entry

A point of entry is how a device connects to the StartWRT network and receives its [Security Profile](security-profiles.md). There are three types of entry points, each mapping to a profile.

## Ethernet

Each physical Ethernet port on the router maps to a Security Profile. The port a device plugs into determines its profile. See [Ethernet](ethernet.md).

## Wi-Fi

StartWRT uses one Wi-Fi network (one SSID) with multiple passwords. Each password maps to a different Security Profile. The password a device uses to join the network determines its profile. See [Wi-Fi](wifi.md).

## Inbound VPN

Each WireGuard VPN server on the router maps to a Security Profile. Remote devices connect to a VPN server and receive the corresponding profile, as if they were physically present on the network. See [Inbound VPNs](inbound-vpn.md).

## Why Entry Points Matter

Traditional routers require you to think in terms of VLANs, firewall rules, and subnets. StartWRT replaces all of that with a simple mental model: **how you connect determines what you can access**. Whether a device plugs into a specific Ethernet port, uses a specific Wi-Fi password, or connects through a specific VPN server, the result is the same — it gets assigned a profile that governs its network access.
