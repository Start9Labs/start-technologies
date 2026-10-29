---
user-facing: true
---

- Configure HTTP proxy authentication and forwarded headers under `bindPort`'s `proxy` option, independently of `addSsl`. Proxy authentication applies to both the original and TLS-wrapper legs.
- Inherit proxy defaults from the protocol: enabled for HTTP-family protocols, disabled otherwise. Every protocol accepts an explicit `proxy` object or `proxy: null` override.
- Move `addSsl.auth` to `proxy.auth` and `addSsl.addXForwardedHeaders` to `proxy.addXForwardedHeaders` when rebuilding packages. Keep TLS settings under `addSsl`.
- Require StartOS 0.4.1 or later for packages built with this SDK.
