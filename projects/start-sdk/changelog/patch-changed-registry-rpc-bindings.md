---
release-notes: false
---

- Refresh bundled transport profiles, registry access, OS-version-aware subscription, RPC response synchronization, typed initialization/setup progress, log and metrics follow, checked log boot selectors, setup password/recovery input types, and terminal command argument compatibility.
- Provide opaque, unpersisted request-proof signing for startup admission without WebCrypto enrollment.
- Validate admission CBOR wire kinds before schema decoding; expose asynchronous first-object decoding with exact captured-byte boundaries.
