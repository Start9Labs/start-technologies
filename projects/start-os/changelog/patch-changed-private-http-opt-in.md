---
release-notes: true
---

- Enable a private plain-HTTP address for a client that cannot trust the server's Root CA without marking an entire gateway secure. Enabling requires an explicit `--allow-insecure` acknowledgement through CLI/RPC/MCP; enabled addresses appear in the UI and can be disabled there. Plaintext traffic, including passwords, can be read and altered by devices on that network.
- Keep disabled private domains switched off in the UI after changing a shared SSL listener's port.
