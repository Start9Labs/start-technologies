---
release-notes: false
---

- Re-adding an SSH key updates its existing entry, keeping its creation date, instead of duplicating its row. The SSH key list stays synchronized across browser sessions and CLI changes.
- SSH key changes wait for authorized-key updates before reporting success.
