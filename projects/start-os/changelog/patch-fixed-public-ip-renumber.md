---
release-notes: true
---

- A public address you enabled stays enabled when its gateway's public IP changes. After a new lease, a moved IPv6 prefix, or a dropped connection, the gateway's new public address keeps the setting and its port forward instead of reverting to off.
- An IPv6 address's on/off and Local/Public settings also apply to other IPv6 addresses on the same gateway with the same interface ID. The gateway table marks these with a link icon.
