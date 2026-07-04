# Outbound VPN

Route your server's outbound Internet traffic through a VPN for privacy. An outbound VPN is like sending your mail through a proxy — the recipient sees the proxy's return address, not yours. Common reasons to use one:

- **Hide your IP address** from external services your server connects to.
- **Prevent ISP monitoring** of your server's traffic.
- **Route sensitive services differently** — for example, send Bitcoin traffic through Mullvad while leaving everything else on the default gateway.

## Add a VPN Gateway

To add an outbound VPN, [add a gateway](gateways.md#adding-a-gateway) using a WireGuard configuration file. There are two options:

|                        | Commercial VPN (Mullvad, ProtonVPN, etc.) | StartTunnel                              |
| ---------------------- | ----------------------------------------- | ---------------------------------------- |
| **Gateway type**       | Outbound only                             | Inbound/outbound                         |
| **Also serves as**     | —                                         | [Inbound VPN](inbound-vpn.md) and [clearnet](clearnet.md) gateway |
| **IP anonymity**       | High — your traffic blends with thousands of other users on shared IPs | Lower — the VPS IP is dedicated to you, so all traffic from it can be correlated |
| **Cost**               | Monthly subscription                      | VPS hosting cost                         |
| **Setup**              | Paste provider's WireGuard config         | See [StartTunnel](/start-tunnel/)        |

Both options hide your home IP address, and in both cases the provider knows who you are. The difference is that a commercial VPN shares IPs across thousands of users, making it harder for external observers to correlate traffic to a specific person. With StartTunnel, the VPS IP is yours alone, so all traffic from it can be linked together. The advantage of StartTunnel is that a single gateway handles both inbound and outbound traffic.

## Set System-Wide Default Gateway

By default, StartOS dynamically selects which gateway to use for outbound traffic for optimal performance ("Auto" mode). You can override this under `System > Gateways > Outbound Traffic` by switching from "Auto" to a specific gateway. This forces _all_ outbound traffic for everything on the server through the selected gateway.

## IPv6 leak prevention

StartOS treats IPv6 outbound routing the same way as IPv4: the default gateway is chosen by route metric, and you can pin all traffic to a specific gateway under `System > Gateways > Outbound Traffic`.

If the gateway you select for outbound traffic can't carry IPv6 — for example a commercial VPN whose WireGuard config has no IPv6 address — StartOS **drops** the server's outbound IPv6 rather than letting it fall back to your ISP connection, so your real IPv6 address never leaks around the VPN. (The drop is a blackhole in that gateway's own routing table.) A gateway that does provide IPv6 (such as a StartTunnel with a [delegated prefix](/start-tunnel/ipv6.html)) carries IPv6 normally; on a server with no native ISP IPv6, such a tunnel can also become your IPv6 default before you pin it, so select an outbound gateway explicitly if you want to control which one.

## Route Individual Services Through VPN

You can override the system default on a per-service basis by navigating to a service and going to **Actions > Set Outbound Gateway**. This lets you route individual services through different VPNs while leaving others on the default.

For example, you could route your Bitcoin node through Mullvad for privacy while leaving Nextcloud on the default gateway for better performance.
