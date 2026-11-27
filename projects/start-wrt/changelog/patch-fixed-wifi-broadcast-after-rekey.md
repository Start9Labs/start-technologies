---
release-notes: true
---

- Fixes Wi-Fi devices on the same security profile losing sight of each other after the router has been up for a day: a device that reconnected after the daily Wi-Fi group-key change could no longer receive broadcast traffic, so pings, casting (Chromecast, Google Home), printer discovery and other local-network discovery failed until the router restarted.
