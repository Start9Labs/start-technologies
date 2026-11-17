---
release-notes: false
---

- Mark a toggle as required with `required: true` on `Value.toggle` and `Value.dynamicToggle`: the input validator rejects `false`, and the form does not submit while the toggle is off.
