---
release-notes: false
---

- Index subscriptions accept an optional `language` and receive registry, category, package and OS text already translated for that locale. A language with no translation falls back to each package's declared default locale.
- `start-registry info set-default-locale` sets the locale the registry's own text falls back to.
