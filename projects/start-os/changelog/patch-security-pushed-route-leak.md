---
release-notes: false
---

- Routes pushed by the local network, such as DHCP option 121 routes in the TunnelVision attack (CVE-2024-3661), can no longer send traffic out the LAN when an outbound gateway is selected for StartOS or for a service, including while that gateway's kill switch is engaged. Replies to connections that arrived through a tunnel also stay on that tunnel.
