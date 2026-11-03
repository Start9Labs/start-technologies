---
release-notes: false
---

- Add identity-key endorsements to local v2 packages with `start-cli s9pk sign`, preserving the embedded signature and original archive bytes. Signing again with an existing valid signer leaves the file unchanged.
- Submit local package endorsements when adding registry packages and mirrors, and retain indexed endorsements during promotion.
- Pin v2 sideload requests to the local file's verified signing identities while preserving the server's trust checks.
