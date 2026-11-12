---
release-notes: true
---

- Fixed connections from a device in one profile to a Wi-Fi device in another stalling, which made SSH sessions lag and VS Code Remote-SSH fail to connect. Established connections pass the router's per-packet firewall checks again: wired TCP keeps its full speed, but UDP faster than a few hundred megabits per second, such as a fast QUIC download, is slower than in 1.4.0.
