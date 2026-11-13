---
release-notes: false
---

- Established connections skip the per-packet firewall checks they already passed, which raises the router's forwarding capacity under heavy load. A router where flow offloading was turned off by hand keeps it off.
