---
user-facing: false
---

- **The session API separates signed-in devices from bearer tokens.** `auth.session.list` returns sessions as `{ pubkeys, tokens }`, and `auth.session.kill` takes `pubkeys` and `tokens` in place of `ids`.
