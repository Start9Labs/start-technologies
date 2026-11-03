---
release-notes: false
---

- Registries are managed under **System → Registries**, where you add or remove registries and choose which one provides StartOS updates; the Marketplace's registry selector links there. Adding a registry trusts its new keys for packages; keys it offers later are imported untrusted, and each key a registry adds or stops offering gets its own notification.
- **System → Trusted Keys** shows each key's label, identities, type, package and StartOS-update trust, the registries offering it, and who added it and when. At least one key stays trusted for StartOS updates, and keys a connected registry offers cannot be removed. **Add key** takes one PEM key or GPG export, and a GPG key is labeled with its identity name.
- Packages not signed by a trusted key can be installed or updated from the Marketplace and the Updates tab after confirming the risks. The dialog for a blocked StartOS update links to Trusted Keys.
- Selecting an available update opens the same package details as the Marketplace.
- The StartOS update dialog lists every key that signed the update and whether you trust it for StartOS updates.
- Package details preview up to three trusted signing keys, falling back to known untrusted keys and then unknown keys. **View all** opens the complete signer list when keys are hidden. Selecting a key there, in the StartOS update dialog, in Trusted Keys, or in a notification shows its details, where you can trust or distrust it for packages and StartOS updates.
