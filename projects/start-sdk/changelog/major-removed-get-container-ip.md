---
release-notes: false
---

- Removed `getContainerIp`. Containers cannot reach each other directly; reach dependencies through their bridge address and the OS through `getOsIp`.
