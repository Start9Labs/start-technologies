---
release-notes: true
---

- A public domain on a service port that StartOS forwards without TLS is reachable over IPv6 as well as IPv4.
- A service changing how it binds a port no longer enables a public domain over TLS.
- On a plain port, a public domain and the public addresses it shares now have one state. If some were on and some off, including an IPv6 address left local, they all turn off until you re-enable them, so check any public domain on a plain port after updating.
