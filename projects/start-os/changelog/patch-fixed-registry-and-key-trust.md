---
release-notes: false
---

- Manage registries in System settings and review their proposed package-signing keys before connecting. Later additions are imported disabled and prompt a link to Manage Trust when visiting the registry. Manage Trust shows each key's GPG identity or label, fingerprint, origin, and independent package and OS-update permissions. System keys and keys listed by connected registries can be disabled but not deleted.
- Marketplace package details identify signers by GPG name and email, including disabled keys; unknown signers show their fingerprint. Untrusted-package warnings link to Manage Trust.
