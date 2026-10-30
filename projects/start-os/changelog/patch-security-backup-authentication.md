---
user-facing: true
---

- Strengthened backup encryption authentication and password-protected key wrapping, and removed backup encryption keys from process arguments and recovery errors. Authenticated current references reject substitutions of individual stored records; rollback to a genuine older complete backup view remains possible. Existing backups remain usable for backups and restores until explicit conversion. Conversion rewrites encrypted data, resumes after interruption, and prevents older StartOS releases from reading the converted target.
