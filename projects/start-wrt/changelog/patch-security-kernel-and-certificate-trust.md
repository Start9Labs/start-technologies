---
release-notes: false
---

- Update the router kernel to fix CVE-2026-99284 and CVE-2026-99288 (NoisyTCP), which can allow local takeover or a remotely triggered crash.
- Stop trusting the emSign ECC Root CA - G3 certificate authority, which browsers distrusted after keys for intermediate authorities it signed were compromised.
