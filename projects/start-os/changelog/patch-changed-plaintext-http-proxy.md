---
user-facing: true
---

- Proxy plaintext service connections through StartOS when HTTP proxying is enabled. Applications see StartOS as the TCP peer; client IPs are available through `X-Forwarded-For` when forwarded-header generation is enabled.
