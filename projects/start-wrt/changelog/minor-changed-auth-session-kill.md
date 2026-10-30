---
user-facing: true
---

- **`startwrt-cli auth session kill` takes `--pubkey <key>` for each sign-in to end**, in place of positional session IDs. The `auth.session.kill` RPC takes `pubkeys` in place of `ids`, and `auth.session.list` returns sessions as `{ pubkeys, tokens }`.
