---
release-notes: false
---

- Declare non-empty TCP/UDP protocol sets for single-port and range bindings. Arrays are deduplicated and sorted; a protocol string selects a singleton set. HTTP/HTTPS, WS/WSS, and SSH default to TCP; DNS, custom protocols, and ranges default to both. Explicit UDP with TLS or HTTP proxy options is rejected. Packages default to requiring StartOS 0.4.3 for enforcement.
