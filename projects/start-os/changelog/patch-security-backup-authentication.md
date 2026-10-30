---
user-facing: true
---

- Strengthened backup encryption authentication and password-protected key wrapping, and removed backup encryption keys from process arguments and recovery errors. Authenticated current references reject individual object/version and log substitutions; rollback to a genuine older complete backup view remains possible. Existing backups are upgraded on writable open; the resumable one-time upgrade rewrites encrypted data and prevents older StartOS releases from reading the upgraded target.
