---
release-notes: false
---

- An action's handler receives only the fields its input form declares; any other key in a submitted input is dropped, at every nesting level. A union field's input carries only its selected variant.
