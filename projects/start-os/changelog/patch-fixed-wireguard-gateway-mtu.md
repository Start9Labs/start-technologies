---
release-notes: false
---

- WireGuard gateways now honor the `MTU` setting in their configuration, both when added and when their configuration is updated. Configurations without it still use the automatic MTU. Updating a gateway's configuration to a different MTU briefly reconnects it.
