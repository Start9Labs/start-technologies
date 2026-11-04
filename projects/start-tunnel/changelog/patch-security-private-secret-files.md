---
release-notes: true
---

- Only root can now read the tunnel database and WireGuard configuration, which hold the server's keys. Existing installs are tightened when the service starts. Thanks to dritzps for reporting this.
