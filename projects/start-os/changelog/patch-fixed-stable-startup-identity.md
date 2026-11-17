---
release-notes: false
---

- Keep the server identity stable across initialization and reboot by persisting its key in boot configuration.
- Preserve encrypted diagnostics when that key is unavailable; temporary recovery identities cannot receive normal UI requests or replace remembered fingerprints.
- Prevent fresh accounts from inheriting another account's identity after interrupted restore or reset.
