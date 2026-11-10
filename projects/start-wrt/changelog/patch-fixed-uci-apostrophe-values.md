---
release-notes: true
---

- StartWRT now reads settings that contain an apostrophe or a backslash the same way OpenWrt does. Previously, a Wi-Fi name like "Rich's Wi-Fi" saved through LuCI, or a profile name with an apostrophe once a WAN schedule had run, left StartWRT unable to read that configuration file.
