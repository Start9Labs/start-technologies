---
release-notes: false
---

- **Outbound VPNs size their MTU to fit their connection.** Each VPN uses the largest MTU that fits through your WAN connection or the VPN it connects via, recalculated whenever either changes, and shows it in its MTU field. An MTU in an imported config file is kept only when it is smaller.
