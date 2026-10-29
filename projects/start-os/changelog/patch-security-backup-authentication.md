---
user-facing: true
---

- Strengthened backup encryption authentication and password-protected key wrapping, and removed backup encryption keys from process arguments and recovery errors. Existing backups are upgraded on writable open; the one-time upgrade rewrites encrypted data and prevents older StartOS releases from reading the upgraded target.
