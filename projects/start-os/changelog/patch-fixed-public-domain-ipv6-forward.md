---
release-notes: false
---

- A public domain on a service port that StartOS forwards without TLS is reachable over IPv6 as well as IPv4.
- A service changing how it binds a port no longer enables a public domain over TLS.
- A public domain newly added to a plain port follows the public addresses already there: it turns on if they were all on and off if they were all off. If they were mixed, everything public on that port turns off.
