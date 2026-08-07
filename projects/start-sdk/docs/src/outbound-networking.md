# Outbound Network Access

[Interfaces](interfaces.md) covers traffic **inbound** to your service. [Service-to-Service Networking](service-to-service.md) covers reaching **another package**. This page covers the third direction: your service reaching the **internet**.

Outbound connections work by default. A service container can open a TCP or UDP connection to any external host — an upstream API, a package registry, a block explorer, an SMTP relay — with no manifest flag and no SDK call.

Three things are blocked, and one of them is the reason most packages that "can't reach the internet" actually fail.

## DNS must go through the container's own resolver

Every service container ships an `/etc/resolv.conf` containing exactly:

```
nameserver 10.0.3.1
```

That is the OS itself, on the bridge gateway, running a recursive resolver on UDP and TCP port 53. It answers `.startos` and private names locally and forwards everything else to whatever upstream resolvers the box is configured with, preserving the query type — so `TXT`, `SRV` and `MX` lookups all work.

Use it. Any library that reads `/etc/resolv.conf` — `getaddrinfo`, Go's resolver, `httpx`, `requests`, `reqwest`, Node's `dns` — is already correct and needs no configuration.

> [!IMPORTANT]
> A DNS query sent from a container **straight at a public resolver or the LAN gateway** — `1.1.1.1:53`, `8.8.8.8:53`, `192.168.1.1:53` — is dropped. Not rejected: dropped. The caller sees no error, only a timeout, once per configured nameserver.

This bites when an upstream application hardcodes a resolver rather than reading the system one. The tell is distinctive: HTTPS to the same domain works fine, but a direct DNS lookup hangs for the client's full timeout and then reports a timeout rather than `NXDOMAIN` or "no nameservers". If a service resolves nothing while its other network calls succeed, check for a hardcoded nameserver list before anything else.

The fix belongs in the application: drop the override and let the resolver library read `/etc/resolv.conf`. If the upstream needs to keep a configurable nameserver for other platforms, have it default to the system resolver and treat an explicit list as an opt-in override — that is correct on StartOS and on every other runtime.

Verify inside a running container with:

```
start-cli package attach <id> -- cat /etc/resolv.conf
```

> [!NOTE]
> The OS resolver forwards queries but does not currently pass DNSSEC records through — responses carry no `RRSIG` and no `AD` flag. A package that must validate DNSSEC itself (BIP 353 payment instructions, for example) cannot do so through it today.

## Port mapping is reserved to the OS

NAT-PMP/PCP (`udp/5351`) and UPnP SSDP (`udp/1900`) are dropped from containers. Only StartOS may open ports on the user's router, and it does so on the user's behalf when they enable clearnet access for an interface. A package must never try to map its own port — declare the binding in [interfaces.ts](interfaces.md) and let the OS decide how it is reached.

## Don't assume how you are reached

Outbound access says nothing about inbound. Whether your service has a Tor address, a clearnet domain, or LAN only is the user's choice at runtime, and your package cannot know it — see the note in [Interfaces](interfaces.md). If the service needs to know its own public URL, use [Set a Primary URL](recipe-primary-url.md).
