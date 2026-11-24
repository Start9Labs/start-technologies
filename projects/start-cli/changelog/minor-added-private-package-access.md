---
release-notes: false
---

- Publish private packages: `s9pk publish --private --reader <KEY>` and `registry package add --private` index an s9pk that is hosted encrypted and visible only to admins, the package's publishers, and the listed reader keys. `s9pk encrypt` writes the ciphertext for hosting elsewhere.
- Manage a registry's labeled reader keys with `registry admin reader` commands.
- Installs and `registry package download` decrypt private s9pks. `registry package promote` carries each s9pk's privacy and readers to the destination registry, and `registry package version export` prints a version as JSON, including reader keys for admins and the package's publishers.
