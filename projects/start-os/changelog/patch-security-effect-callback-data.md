---
release-notes: true
---

- Prevent service-effect callback notifications from exposing unrelated database state. Callbacks carry only an invalidation signal; services reread values through the requested effect.
