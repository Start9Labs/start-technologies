---
release-notes: true
---

- Public domains on services that StartOS forwards directly, without securing the connection itself, now work over IPv6 as well as IPv4.
- A service update can no longer turn on one of your public domains by itself.
- On those services, a public domain and the public IP addresses it uses now turn on and off together. If some were on and others off, all of them turn off after this update, so a domain that was working may stop until you turn it back on. Check your public domains after updating.
