---
release-notes: false
---

- Poll shared-volume and backup-target observation futures on one RPC-owned task using tiered loops. Preserve pending I/O and per-source backoff across observer-session retries.
