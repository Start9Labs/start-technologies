---
release-notes: false
---

- Added full-disk-encryption enrollment settings with explicit restart/password acknowledgment, firmware approval codes and certificate fingerprints. Approved firmware enrollment prepares recoverable administrator credentials while preserving legacy access pending activation; incomplete preparation survives password changes and retained-key loss. Firmware enrollment is shown separately from disk protection.
- FDE enrollment keeps ZRAM off while signing keys or encrypted-disk credentials are managed, restoring its previous availability after a rejected request with no persisted authority. Credential protection precedes database migrations and key repair.
- Protected boot presents a restricted HTTPS password-unlock page before normal server initialization, keeping SSH and normal system-service activation offline until unlock. Enrollment alone does not activate protected boot.
