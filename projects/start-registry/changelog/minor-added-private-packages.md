---
user-facing: true
---

- **Packages can be private.** A private package is left out of `package get`, `package index`,
  `index`, and `package signer list` except for requests signed by an admin, a signer authorized for it, or one of its
  readers.
  `package access set-private <ID> true` marks a package private, even before its first upload;
  `package access add-reader`, `remove-reader`, and `get` manage its readers.
  `/icons/` serves a private package's icons only to requests signed by those same signers.
  `db.subscribe` streams only the packages its subscriber can see.
