---
release-notes: false
---

- Prevent clients from passing a forged address to services in `X-Real-IP`, `True-Client-IP`, `CF-Connecting-IP` or `Forwarded` headers.
- Refuse WebSocket connections that another website opens to a password-protected service interface.
