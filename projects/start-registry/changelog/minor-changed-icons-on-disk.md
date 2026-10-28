---
user-facing: true
---

- **Package icons are stored in the registry's data directory under `icons/`** rather than in the
  index, so the index and its `db.subscribe` streams no longer carry them. Existing icons move there
  on the first start. `package get` still includes icons for StartOS before 0.4.0.3.
