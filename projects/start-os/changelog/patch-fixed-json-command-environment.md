---
user-facing: true
---

- Accept subcontainer environment overrides through numbered `JSON_ENV_*` variables, including multiline values and variable removal.
- Preserve multiline image environment defaults in `package attach`.
- Respect explicit removal of `HOME` without restoring its default.
