---
release-notes: false
---

- Declare TCP, UDP, or both for single-port and range bindings. HTTP/HTTPS, WS/WSS, and SSH default to TCP; DNS, custom protocols, and ranges default to both. Explicit UDP with TLS or HTTP proxy options is rejected. Packages default to requiring StartOS 0.4.3 for enforcement.
