---
release-notes: true
---

- Strip client-supplied `Forwarded` and `X-Real-IP` headers at HTTP proxy ingress, alongside `X-Forwarded-*`, to prevent applications and proxy plugins from trusting spoofed client metadata.
