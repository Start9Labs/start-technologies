---
user-facing: true
---

- **Published ports save while the router has no global IPv6 address.** Losing the WAN's global IPv6 address blocked every published-port change, including IPv4-only ones, once any IPv6 forward existed. Existing IPv6 forwards keep their rule and show that the router has no global IPv6 address; only adding an IPv6 forward still needs one.
