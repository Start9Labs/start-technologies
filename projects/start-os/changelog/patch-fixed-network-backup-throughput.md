---
user-facing: true
---

- Backups to network shares are much faster, especially over high-latency links such as remote sites reached through StartTunnel. Backup data is now written in large batches with many requests in flight, instead of waiting for a network round trip on each small write.
