# Outbound Traffic

Outbound traffic is every connection your server starts: browsing registries, downloading services and OS updates, a service syncing a blockchain or sending email. Outbound Traffic settings choose which [gateway](gateways.md) carries those connections and which DNS servers resolve the names they look up.

How others reach your server is configured separately, on each service interface. See [Interfaces](interfaces.md).

## Where to Find It

- **System** — `System > Outbound Traffic`, directly below **Gateways**. Applies to the OS itself and to every service set to **Use system gateway**.
- **Service** — a service's **Outbound Traffic** tab, directly above **Actions & Config**. Applies to that service alone.

Each page has two settings, **Gateway** and **DNS**, which are saved together.

## Gateway

On the system page, **Gateway** is **Auto** or a specific gateway. **Auto** selects the physical connection with the preferred default route, preferring IPv4 and using IPv6 when no physical IPv4 default route is available.

On a service's page, **Gateway** is **Use system gateway** or a specific gateway. **Use system gateway** follows the system setting, whether that is **Auto** or a pinned gateway. A specific gateway always takes precedence over the system setting.

Changing or clearing a service's gateway drops its established outbound connections. The service reconnects as needed through the newly selected gateway.

## DNS

**DNS** is **Use Gateway DNS** or **Static**.

- **Use Gateway DNS** resolves through the selected gateway's [DNS settings](dns.md#gateway-dns).
- **Static** resolves through servers you supply, for that requester alone.

Either way, queries pass through the OS DNS server and travel through the selected gateway. An unavailable gateway blocks them.

## How System and Service Settings Interact

A service's DNS follows its own gateway, never the OS's DNS. The OS's **Static** DNS servers apply to the OS only.

| Service Gateway        | Service DNS     | The service's traffic and DNS use                                       |
| ---------------------- | --------------- | ----------------------------------------------------------------------- |
| **Use system gateway** | Use Gateway DNS | the system's gateway, resolving through that gateway's own DNS settings |
| **Use system gateway** | Static          | the system's gateway, resolving through the service's servers           |
| A specific gateway     | Use Gateway DNS | that gateway, resolving through its own DNS settings                    |
| A specific gateway     | Static          | that gateway, resolving through the service's servers                   |

For example, with the system pinned to Mullvad and its DNS set to **Static**, a service on **Use system gateway** and **Use Gateway DNS** goes through Mullvad and resolves through Mullvad's DNS settings, not the OS's static servers.

## Leak Prevention

A selected gateway acts as a kill switch. If it disconnects, StartOS rejects the traffic assigned to it instead of rerouting that traffic through another gateway. A system-wide selection protects the OS and every service that follows it; a service's selection protects that service. Marketplace access, service downloads, and OS updates remain offline while the system's selected gateway is unavailable. Your LAN, and connections that reach the server through any other gateway, keep working.

A gateway carries IPv6 as well as IPv4. If the selected gateway can't carry IPv6 — for example a commercial VPN whose WireGuard config has no IPv6 address, or a LAN whose router advertises IPv6 without assigning your server an address — StartOS **drops** that outbound IPv6 rather than letting it fall back to your ISP connection, so your real IPv6 address never leaks around the VPN. A gateway that does provide IPv6, such as a StartTunnel with a [delegated prefix](/start-tunnel/ipv6.html), carries IPv6 normally, including on a server with no native ISP IPv6.

For its own connections — marketplace, service and OS-update downloads — StartOS tries a host's IPv6 and IPv4 addresses a quarter of a second apart and uses the first connection that succeeds. Services make their own connections and follow their own behavior.

## Recommended Settings

- **Most servers:** leave the system on **Auto** and every service on **Use system gateway**, with DNS on **Use Gateway DNS** throughout.
- **Privacy for particular services:** add an [outbound VPN](outbound-vpn.md) and select it on those services' **Outbound Traffic** tabs. For example, route a Bitcoin node through Mullvad while leaving Nextcloud on your home connection for better performance.
- **Privacy for everything:** select the VPN on the system page. The OS's own connections — registries, service downloads, OS updates — then go through it too, along with every service on **Use system gateway**.
- **A specific DNS provider:** set it on the gateway, under [Static DNS Servers](dns.md#static-dns-servers), so everything using that gateway follows it. Reserve **Static** on an Outbound Traffic page for one requester that needs different servers from everything else on its gateway.
