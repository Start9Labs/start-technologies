---
release-notes: true
---

- A public address you enabled stays enabled when its gateway's public IP changes. After a new lease, a moved IPv6 prefix, or a dropped connection, the replacement address keeps the setting and its port forward instead of reverting to off. A disconnected gateway keeps listing its last public addresses until it reconnects or is removed.
