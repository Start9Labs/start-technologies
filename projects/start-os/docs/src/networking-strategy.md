# Networking Strategy

StartOS supports multiple networking strategies that can be used alone or in combination. Choosing the right one depends on your goals — but first, it helps to separate three concepts that are often conflated: **security**, **privacy**, and **censorship resistance**. These are independent properties, and each networking strategy offers a different mix of them.

## Security, Privacy, and Censorship Resistance

**Security** means your data is encrypted in transit and only accessible to authorized parties. Every networking strategy on StartOS uses encryption (TLS, WireGuard, or Tor circuits). A service hosted on the clearnet with HTTPS is just as encrypted in transit as one accessed over Tor. Every major password manager, bank, and email provider operates on the clearnet — because transport encryption and strong authentication (passwords, 2FA) are what make a connection secure, not the network layer.

**Privacy** means observers cannot easily determine who is hosting or accessing a service. This is where the strategies diverge. Clearnet hosting reveals the gateway's IP address, which maps to an approximate geographic location. Tor hides that information. VPN access is private because only authorized devices can reach the service at all.

**Censorship resistance** means a service cannot be taken offline by third parties — domain registrars, DNS providers, ISPs, or governments. Tor onion services are the strongest option here, since they rely on no centralized infrastructure. Clearnet services depend on DNS and domain registration, which can be seized or blocked. VPN is resistant to censorship but is primarily designed for private access rather than public hosting.

|              | Security                   | Privacy                                                                                                                       | Censorship Resistance                        |
| ------------ | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| **Clearnet** | Strong (HTTPS/TLS)         | Low (router) / Medium ([StartTunnel](/start-tunnel/)) — gateway IP is public, but StartTunnel hides your home IP behind a VPS | Low — depends on DNS and domain registrar    |
| **Tor**      | Strong (onion routing)     | High — hides both server and client                                                                                           | High — no DNS, no domain, no port forwarding |
| **VPN**      | Strong (WireGuard/OpenVPN) | High — service not publicly visible                                                                                           | N/A — private access only                    |

## Common Misconceptions

### "Clearnet is insecure"

This is the most common mistake. Hosting Vaultwarden, Nextcloud, or any other service on the clearnet with HTTPS is not insecure. The connection is encrypted with the same TLS that protects every bank and password manager on the Internet. What matters is:

- **Strong, unique passwords** for every account
- **Two-factor authentication** (2FA) wherever supported
- **Keeping software updated** to patch known vulnerabilities

What clearnet _does_ trade away is a degree of privacy (your gateway's IP is visible) and censorship resistance (your domain can theoretically be seized). Using a [StartTunnel](/start-tunnel/) gateway significantly mitigates the privacy concern — visitors see the VPS IP, not your home IP. For most personal hosting scenarios, these are acceptable tradeoffs.

### "Tor is more secure than clearnet"

Tor adds **privacy** and **censorship resistance**, not additional transport security. An HTTPS connection over clearnet and an HTTP connection over Tor are both encrypted end-to-end. In fact, Tor's `.onion` connections are inherently encrypted, which is why [SSL is optional](tor.md#http-vs-https-ssl) for onion services. Tor is the right choice when anonymity or censorship resistance matters — not because clearnet is insecure.

## Choosing a Strategy

### Who needs access?

- **Only you and your devices** → This is [private access](private-access.md). Use [LAN](lan.md) at home, and [VPN](inbound-vpn.md) or [Tor](tor.md) remotely.
- **Other people too** → This is [public access](public-access.md). Use [clearnet](clearnet.md), [Tor](tor.md), or both.

### For private access: VPN, Tor, or Clearnet?

|                             | Speed | Setup                          | Works behind CGNAT                      | Requires Root CA trust | Client software required |
| --------------------------- | ----- | ------------------------------ | --------------------------------------- | ---------------------- | ------------------------ |
| **[VPN](inbound-vpn.md)**   | Fast  | Medium (router or StartTunnel) | Only with [StartTunnel](/start-tunnel/) | Yes                    | Yes                      |
| **[Tor](tor.md)**           | Slow  | Low                            | Yes                                     | No                     | Yes (Tor browser)        |
| **[Clearnet](clearnet.md)** | Fast  | High (domain, DNS, gateway)    | Only with [StartTunnel](/start-tunnel/) | No                     | No                       |

- **VPN** gives you seamless access — every service is reachable at its LAN IP as if you were home. Setup involves configuring a VPN server (on your router or via [StartTunnel](/start-tunnel/)), [trusting your Root CA](trust-ca.md), and installing a client on every device.
- **Tor** is the right choice when you need access from an untrusted device or network where you don't want to install a VPN client, when you're behind [CGNAT](cgnat.md) and don't want to set up StartTunnel, or when anonymity matters.
- **Clearnet** can also work well for private access. Many people use clearnet URLs with strong passwords and 2FA to access their own services — exactly the way you'd use any cloud-hosted app. The StartOS dashboard also offers rate limiting for additional protection. This trades some privacy (gateway IP is visible) for maximum convenience — no client software needed, just a browser.

### For public access: Clearnet, Tor, or both?

|                             | Audience reach            | Setup                       | Privacy                  | Censorship resistance               |
| --------------------------- | ------------------------- | --------------------------- | ------------------------ | ----------------------------------- |
| **[Clearnet](clearnet.md)** | Anyone with a browser     | High (domain, DNS, gateway) | Low (gateway IP visible) | Low                                 |
| **[Tor](tor.md)**           | Anyone with a Tor browser | Low                         | High                     | High                                |
| **Both**                    | Maximum reach             | Highest                     | Mixed                    | Clearnet can be blocked; Tor cannot |

- **Clearnet** is the natural choice for services meant to be easily accessible — a family Nextcloud, a personal website, a Nostr relay. Standard browsers work without any special software. Use a [StartTunnel](/start-tunnel/) gateway if you want to avoid exposing your home IP.
- **Tor** is the right choice for services where anonymity or censorship resistance is a priority, or when you want a simple way to host publicly without purchasing a domain.
- **Both together** give you a standard domain for everyday access and a `.onion` fallback that cannot be censored.

## Example Setups

### Personal server (Vaultwarden, Nextcloud, Jellyfin)

- **At home**: [LAN](lan.md) — fastest, no Internet involved
- **Away from home**: [VPN](inbound-vpn.md) — fast, private, all services reachable
- **Sharing with family**: [Clearnet](clearnet.md) with a domain — they use it like any other website, with strong passwords and 2FA

### Bitcoin and Lightning node

- **Bitcoin P2P**: [Public IP](public-ip.md) — lets your node participate in the network
- **Lightning**: [Public IP](public-ip.md) — required for routing and receiving payments
- **Personal access to dashboards**: [VPN](inbound-vpn.md) or [LAN](lan.md) — keep admin interfaces private

### Anonymous or censorship-resistant hosting

- **Primary access**: [Tor](tor.md) — publish the `.onion` address, no domain or DNS needed
- **Optional clearnet mirror**: [Clearnet](clearnet.md) with a [StartTunnel](/start-tunnel/) gateway — hides your home IP behind the VPS

### Simple setup, no network control

- **All services**: [Tor](tor.md) — works from any Tor-enabled browser, no router configuration needed
