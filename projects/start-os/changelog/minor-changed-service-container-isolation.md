---
release-notes: true
---

- Services can no longer reach each other's containers directly; they connect through StartOS, as dependencies already do. Each service container also runs under its own range of user and group ids.
- A server holds up to 1024 installed services. Installing beyond that is refused before anything is downloaded.
- StartOS no longer runs its own SOCKS proxy on port 1080. Services that need Tor use the Tor service's proxy.
- The StartOS UI serves plain HTTP only on private addresses that are switched on, never on public ones. They are on by default on every network, so downloading the Root CA over your LAN keeps working, and each can be switched off. IPv6 link-local addresses no longer serve the UI.
- Services' plain ports are reachable on their IPv6 ULA and loopback addresses.
