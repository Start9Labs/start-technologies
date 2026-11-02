---
release-notes: false
---

- Registries are listed in the Marketplace sidebar and managed under **System → Registries**, where you add or remove registries and choose which one provides StartOS updates. Adding a registry trusts its new keys for packages; keys it offers later are imported untrusted and prompt a review when you visit it.
- **System → Trusted Keys** shows each key's label, identities, type, package and StartOS-update trust, the registries offering it, and who added it and when. At least one key stays trusted for StartOS updates, and keys a connected registry offers cannot be removed.
- Marketplace package details identify trusted package signers by label and email, falling back to known untrusted signers only when no trusted signer matches. Unknown signers are hidden. Untrusted-package warnings link to Trusted Keys.
