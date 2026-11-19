---
release-notes: false
---

- `getContainerIp` takes no `packageId` and returns only the calling service's own container address, which is fixed for the installation. Containers cannot reach each other directly; reach dependencies through their bridge address and the OS through `getOsIp`.
