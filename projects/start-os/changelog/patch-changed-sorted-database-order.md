---
release-notes: false
---

- Health checks, service interfaces and tasks keep the order the service defines. Gateways, SSH keys, shared volumes, backup targets, certificate authorities, registries and updates are listed in a consistent order that no longer changes after reloading the page.
- When shared volumes have duplicate names, upgrading can change which one keeps the original name and which ones receive suffixes.
- Adding a public domain defaults to Let's Encrypt whenever it is one of the configured certificate authorities.
