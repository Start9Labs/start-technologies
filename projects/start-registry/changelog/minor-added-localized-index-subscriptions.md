---
release-notes: false
---

- Index subscriptions accept an optional `language` and receive registry, category, package and OS text already translated for that locale. A language with no translation falls back to each package's declared default locale. Subscriptions restart when available languages change, letting reconnecting clients select newly published translations.
- `start-registry info set-default-locale` sets the locale the registry's own text falls back to.
