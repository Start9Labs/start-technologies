---
release-notes: false
---

- Prevent clients from passing a forged address to services in `X-Real-IP`, `True-Client-IP`, `CF-Connecting-IP` or `Forwarded` headers.
- Refuse WebSocket connections and state-changing requests, such as form submissions, that another website sends to a service interface protected by HTTP Basic authentication.
