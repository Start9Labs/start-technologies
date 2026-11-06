# Wireless WAN

Wireless WAN connects the router to another Wi-Fi network and uses it for Internet access, as a backup to the WAN port or in its place. Navigate to `Internet > Wireless WAN`.

## How it works

The router joins the upstream network as a Wi-Fi client on one of its bands. Your devices reach the Internet through it exactly as through the WAN port: [security profiles](security-profiles.md) that use the WAN, [published ports](published-ports.md) and remote access all follow whichever connection is carrying traffic. The router uses the DNS servers the upstream network offers, unless you have set custom servers under [WAN Settings](wan.md#dns).

## Joining a network

1. Choose the band to connect on, if both are on.
1. Click **Scan**. The list shows each network's name, signal strength, security and channel.
1. Click **Join** beside the network, or **Hidden network** to enter the name of one that does not broadcast it.
1. Enter the password and choose a priority, then click **Join**.

WPA2, WPA3 and open networks are supported. WPA Enterprise and WEP networks are listed as unsupported.

> [!WARNING]
> The band that carries the uplink belongs to it. Its Wi-Fi follows the upstream network's channel, and it is off whenever the router is not connected to that network: while it is joining, after a wrong password, or when the network is out of range. Join from a wired device or from the other band, so you do not lose access to the router.

## Priority

- **Backup** (default) — The WAN port carries traffic whenever it has a connection. The Wi-Fi uplink takes over when the WAN port loses its link.
- **Preferred** — The Wi-Fi uplink carries traffic whenever it is connected. The WAN port takes over when it drops.

The router switches when a connection goes down, not when an upstream network stops reaching the Internet while staying connected. A WAN port with a [static address](wan.md#ipv4) stays up without a cable, so it never hands over to a backup uplink.

The **Summary** shows whether the router is connected, the address the upstream network assigned, the signal, and whether the uplink is **In use** or **Standing by**.

## Removing the uplink

Click **Remove uplink** in the **Summary**. The router leaves the network and its Internet access depends on the WAN port alone.

## Interactions

- **Wi-Fi settings** — The uplink's band cannot be turned off on the [Wi-Fi](wifi.md) page while the uplink exists. Its channel setting has no effect until the uplink is removed.
- **Wi-Fi Blackout** — A [blackout window](wifi-schedules.md) turns every radio off, including the one carrying the uplink.
- **LuCI** — A Wi-Fi uplink set up in LuCI is shown here but not managed. Delete it in LuCI to manage the uplink from this page.
- **IPv6** — The Wi-Fi uplink carries IPv4. IPv6 stays on the WAN port.
- **Dynamic DNS** — [Dynamic DNS](ddns.md) publishes the WAN port's address.
