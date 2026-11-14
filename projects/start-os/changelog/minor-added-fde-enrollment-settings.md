---
release-notes: false
---

- Added full-disk-encryption enrollment settings with explicit restart/password acknowledgment, firmware approval codes and certificate fingerprints. Firmware enrollment is shown separately from disk protection.
- FDE enrollment keeps ZRAM off while signing keys or encrypted-disk credentials are managed.
- Protected boot presents a restricted HTTPS password-unlock page before normal server initialization, keeping SSH and normal system-service activation offline until unlock. Enrollment alone does not activate protected boot.
