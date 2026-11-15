---
release-notes: true
---

- A Security Profile can no longer use the subnet of your Internet connection, whether you add or edit the profile, change the Network Block, or set a static WAN address. New profiles also start on a subnet that is free of your Internet connection and of the ones most home routers use, such as `192.168.3.x`. If your Internet router's subnet later matches one of your profiles, StartWRT shows a warning on every page and in the Activity log, and suggests a free subnet. Before, the first profile you added behind a router at `192.168.1.1` took `192.168.1.x`, and the Internet dropped out every few minutes.
