---
release-notes: false
---

- After a required check succeeds, dependent daemons tolerate up to two consecutive `failure` polls. Other non-success statuses and daemon crashes or unexpected exits pause dependents immediately. Tolerance counts polls, not a fixed timeout.
- Ready checks with `display: null` log their failures.
- Ready checks continue polling after synchronous throws, non-`Error` throws, and failures to publish health.
