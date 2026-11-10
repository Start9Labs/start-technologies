---
release-notes: false
---

- `merge()` rebuilds a file model's file from its schema defaults when the file cannot be parsed, instead of throwing, so a hand edit that breaks the syntax no longer stops the service from starting.
