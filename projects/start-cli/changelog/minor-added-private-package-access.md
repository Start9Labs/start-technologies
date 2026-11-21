---
release-notes: false
---

- Manage registry package privacy and reader permissions with `registry package access` commands. `registry package access restrict` makes a package private to exactly the registered signers holding the given keys.
- `registry package promote` keeps a private package private on the destination registry, with the same readers, registering any reader the destination does not know yet.
