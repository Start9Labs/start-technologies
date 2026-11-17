---
release-notes: false
---

- Added full-disk-encryption enrollment settings with explicit restart/password acknowledgment, firmware approval codes and certificate fingerprints. Approved firmware enrollment prepares recoverable administrator credentials while preserving legacy access pending activation; incomplete preparation survives password changes and retained-key loss, and retries repair console credentials after interrupted password changes. Firmware enrollment is shown separately from disk protection.
- A fresh acknowledged retry after firmware approval captures configuration through a native filesystem read epoch and stages an unpublished signed candidate with private credential projection. Database readers remain available while persistence writers pause. Native OS maintenance shares image-store ownership; boot configuration references survive credential handoff and daemon restart. Candidate preparation is not public builder-source admission or EFI activation.
- FDE enrollment keeps ZRAM off while signing keys or encrypted-disk credentials are managed, restoring its previous availability after a rejected request with no persisted authority. Credential protection precedes database migrations and key repair.
- Protected boot presents a restricted HTTPS password-unlock page before normal server initialization, keeping SSH and normal system-service activation offline until unlock. Enrollment alone does not activate protected boot.
