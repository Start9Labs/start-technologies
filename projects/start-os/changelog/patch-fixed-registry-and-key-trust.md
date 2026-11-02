---
release-notes: false
---

- Registries are managed under **System → Registries**, where you add or remove registries and choose which one provides StartOS updates. Adding a registry trusts its new keys for packages; keys it offers later are imported untrusted, and a notification offers to trust them.
- **System → Trusted Keys** shows each key's label, identities, type, package and StartOS-update trust, the registries offering it, and who added it and when. At least one key stays trusted for StartOS updates, and keys a connected registry offers cannot be removed.
- Packages not signed by a trusted key can be installed or updated from the Marketplace and the Updates tab after confirming the risks. Their warnings link to Trusted Keys, as does the dialog for a blocked StartOS update.
- Marketplace package details identify trusted package signers by label and email, falling back to known untrusted signers only when no trusted signer matches. Unknown signers are hidden.
