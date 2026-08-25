# Outbound VPN

Route your server's outbound Internet traffic through a VPN for privacy. An outbound VPN is like sending your mail through a proxy — the recipient sees the proxy's return address, not yours. Common reasons to use one:

- **Hide your IP address** from external services your server connects to.
- **Prevent ISP monitoring** of your server's traffic.
- **Route sensitive services differently** — for example, send Bitcoin traffic through Mullvad while leaving everything else on the default gateway.

## Add a VPN Gateway

To add an outbound VPN, [add a gateway](gateways.md#adding-a-gateway) using a WireGuard configuration file. There are two options:

|                    | Commercial VPN (Mullvad, ProtonVPN, etc.)                              | StartTunnel                                                                      |
| ------------------ | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| **Gateway type**   | Outbound only                                                          | Inbound/outbound                                                                 |
| **Also serves as** | —                                                                      | [Inbound VPN](inbound-vpn.md) and [clearnet](clearnet.md) gateway                |
| **IP anonymity**   | High — your traffic blends with thousands of other users on shared IPs | Lower — the VPS IP is dedicated to you, so all traffic from it can be correlated |
| **Cost**           | Monthly subscription                                                   | VPS hosting cost                                                                 |
| **Setup**          | Paste provider's WireGuard config                                      | See [StartTunnel](/start-tunnel/)                                                |

Both options hide your home IP address, and in both cases the provider knows who you are. The difference is that a commercial VPN shares IPs across thousands of users, making it harder for external observers to correlate traffic to a specific person. With StartTunnel, the VPS IP is yours alone, so all traffic from it can be linked together. The advantage of StartTunnel is that a single gateway handles both inbound and outbound traffic.

## Set System-Wide Default Gateway

By default, StartOS dynamically selects which gateway to use for outbound traffic for optimal performance ("Auto" mode). You can override this under `System > Gateways > Outbound Traffic` by switching from "Auto" to a specific gateway. This sets the system-wide default: it covers everything on the server — every service, and the OS itself (registry connections, package downloads) — except services with their own [per-service override](#route-individual-services-through-vpn), which keep their own gateway.

## IPv6 leak prevention

StartOS treats IPv6 outbound routing the same way as IPv4: the default gateway is chosen by route metric, and you can set the system-wide default under `System > Gateways > Outbound Traffic`.

If the gateway you select for outbound traffic can't carry IPv6 — for example a commercial VPN whose WireGuard config has no IPv6 address — StartOS **drops** the server's outbound IPv6 rather than letting it fall back to your ISP connection, so your real IPv6 address never leaks around the VPN. (The drop is a blackhole in that gateway's own routing table.) A gateway that does provide IPv6 (such as a StartTunnel with a [delegated prefix](/start-tunnel/ipv6.html)) carries IPv6 normally; on a server with no native ISP IPv6, such a tunnel can also become your IPv6 default before you pin it, so select an outbound gateway explicitly if you want to control which one.

## Route Individual Services Through VPN

A service's own setting always takes precedence over the system-wide default — whether that default is Auto or a pinned gateway. To set one, navigate to a service and go to **Actions > Set Outbound Gateway**. This lets you route individual services through different VPNs while leaving others on the default; choose "System default" in that dialog to clear the override and follow the system-wide setting again.

For example, you could route your Bitcoin node through Mullvad for privacy while leaving Nextcloud on the default gateway for better performance — even with Mullvad also pinned as the system-wide default.
