---
release-notes: false
---

- `buildManifest` takes named arguments, `{ versionGraph, manifest, dependencies, i18n }`, and now requires the package's `setupI18n` result. Its default language becomes the manifest's `defaultLocale`, the locale StartOS and registries fall back to when a reader's language is missing.
