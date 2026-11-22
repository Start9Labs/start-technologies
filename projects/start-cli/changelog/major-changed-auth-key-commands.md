---
release-notes: true
---

- `auth session list` and `auth session kill` are removed. Manage device keys with `auth key list` and `auth key remove <KEY>...`, and StartOS bearer tokens with `auth token list` and `auth token remove <ID>...`. `auth key add <KEY>` enrolls a device key without a password, optionally labeled with `--name` and `--user-agent`.
