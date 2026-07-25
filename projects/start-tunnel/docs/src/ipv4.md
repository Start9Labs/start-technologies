# IPv4

StartTunnel uses your VPS's public IPv4 addresses for clearnet hosting and
outbound traffic. The one dedicated address required at install time is all
most users need — this page covers adding **more**. Each additional address is
a full, independent public identity with its own port space.

## Why add an address

- **A separate egress identity.** Point a subnet — or a single device — at its
  own address, so its outbound traffic is distinguishable from everything else
  on the tunnel.
- **A second port space.** Over IPv4, an external port can carry only one plain
  forward per address (SNI hostnames on one TLS port being the exception). A
  second address lets two devices each claim, say, port `443` outright.

## Getting an address onto your VPS

Ordering an extra IPv4 attaches it to your server, but does not necessarily
configure it: the network configuration a provider applies at a server's first
boot covers what existed then, and secondary addresses are typically not handed
out by DHCP afterwards. Until the address is actually configured on the host's
interface, StartTunnel cannot see it. Your provider's docs are the authority
for both steps — ordering the address, and configuring it on an existing
Debian server. `ip addr` on the VPS shows what the host holds; the new address
must appear there.

> [!NOTE]
> This is about **additional** addresses. Providers that assign your primary
> address at their edge and give the server a private one (AWS, Google Cloud,
> Azure, Oracle Cloud) are a different arrangement: that address is yours, but
> the server never holds it. StartTunnel egresses and publishes on the address
> the server does hold — the one inbound traffic carries once your provider has
> translated it — so the **WAN IP** selectors and the **External IP** column
> show that address rather than the one you dial.

## Using it in StartTunnel

StartTunnel picks up the host's public IPv4 addresses automatically — no
restart needed. Once the host holds the new address, it shows up:

- as a choice in the **WAN IP** selectors, to be assigned as a subnet's
  [outbound IP](subnets.md#outbound-ip) or a single device's
  [override](devices.md#outbound-ip);
- in **Settings → HTTP Redirect (80 → 443)**, with a port-80 HTTP→HTTPS
  redirect of its own, on by default — see
  [HTTP Redirects](http-redirects.md).

Assigning the address moves that subnet's or device's egress onto it, and a
device's [published ports](published-ports.md) follow: their external IP is
always the device's effective WAN. Configs generated afterwards use it as
their `Endpoint`; existing device configs keep working, because the server
answers on all of its addresses.

## Verifying

- `ip addr` on the VPS lists the new address — the provider-side and host-side
  setup is done.
- The address appears in the **WAN IP** selector of the subnet and device
  dialogs — StartTunnel has detected it.
- After assigning it, an IP-echo site visited from a device on that subnet
  reports the new address — egress is moved.
