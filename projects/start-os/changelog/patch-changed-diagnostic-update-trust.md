---
release-notes: false
---

- Diagnostic updates honor your saved StartOS-update signing keys without needing the data drive. Accepting an untrusted signer requires an authenticated CLI running locally on the server.
- Removed preseeded Alpha build keys from new installations and during migration; connected registries can offer those keys again for explicit trust.
- Local-cookie authentication requires a loopback connection. Remote clients continue to use signed requests.
