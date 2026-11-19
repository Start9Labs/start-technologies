---
release-notes: true
---

- Preserve temporary directories when mount teardown fails or is abandoned, preventing cleanup from deleting still-mounted contents. Nested teardown now handles escaped mount paths through native mount inspection.
