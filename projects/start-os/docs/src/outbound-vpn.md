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

By default, StartOS selects the physical connection with the preferred default route ("Auto" mode), preferring IPv4 and using IPv6 when no physical IPv4 default route is available. You can override this under `System > Outbound Traffic` by switching from "Auto" to a specific gateway. This sets the system-wide default: it covers everything on the server — every service, and the OS itself (registry connections, package downloads) — except services with their own [per-service override](#route-individual-services-through-vpn), which keep their own gateway.

## Leak prevention

A selected gateway acts as a kill switch. If it disconnects, StartOS rejects the traffic assigned to it instead of rerouting that traffic through another gateway. A system-wide selection protects the OS and every service that follows it; a per-service selection protects that service. Choose **Auto** system-wide when you want StartOS to follow the physical default route. Choose **System default** for a service when it should follow the system-wide policy. Marketplace access, package downloads, and OS updates remain offline while their selected system-wide gateway is unavailable. Your LAN, and connections that reach the server through any other gateway, keep working.

You can set the system-wide default for both IPv4 and IPv6 under `System > Outbound Traffic`.

If the gateway you select for outbound traffic can't carry IPv6 — for example a commercial VPN whose WireGuard config has no IPv6 address, or a LAN whose router advertises IPv6 without assigning your server an address — StartOS **drops** the server's outbound IPv6 rather than letting it fall back to your ISP connection, so your real IPv6 address never leaks around the VPN. A gateway that does provide IPv6 (such as a StartTunnel with a [delegated prefix](/start-tunnel/ipv6.html)) carries IPv6 normally, including on a server with no native ISP IPv6.

For its own connections — marketplace, service-package and OS-update downloads — StartOS tries a host's IPv6 and IPv4 addresses a quarter of a second apart and uses the first connection that succeeds. Services make their own connections and follow their own behavior.

The OS and services query the OS DNS server. By default it uses the selected gateway's [DNS settings](dns.md); the adjacent DNS selector also allows an [OS or service-specific static override](dns.md#os-and-service-dns-overrides). These upstreams are reached through the same selected gateway. The Gateways table shows each gateway's resolver addresses and their source. Host lookups of configured WireGuard endpoint names use the automatic physical connection's gateway DNS to allow reconnection; service lookups keep their selected gateway.

## Route Individual Services Through VPN

A service's own setting always takes precedence over the system-wide default — whether that default is Auto or a pinned gateway. To set one, navigate to a service and open its **Outbound Traffic** tab, directly above **Actions & Config**. You can keep one gateway pinned system-wide while routing selected services through another; choose "System default" on that page to clear the override and follow the system-wide setting again. Changing or clearing the override drops the service's established outbound connections. The service reconnects as needed through the newly selected gateway.

For example, you could route your Bitcoin node through Mullvad for privacy while leaving Nextcloud on the default gateway for better performance — even with Mullvad also pinned as the system-wide default.

A service's gateway carries its IPv6 as well as its IPv4, with the same [leak prevention](#leak-prevention) as the system-wide default: if the gateway you choose for a service can't carry IPv6, StartOS drops that service's IPv6.
