---
user-facing: true
---

- Added bearer tokens for API clients that cannot sign requests: `start-cli auth token create --name <name>` prints a token once, and requests sent with `Authorization: Bearer <token>` authenticate as a session with that name, which the session list shows and can revoke. Tokens idle for 30 days are revoked like other sessions.
