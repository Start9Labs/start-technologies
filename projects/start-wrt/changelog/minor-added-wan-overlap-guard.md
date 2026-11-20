---
release-notes: true
---

- A security profile whose subnet overlaps the network the WAN is on no longer knocks the router offline. StartWRT moves Admin, or a profile with nothing assigned to it, to a free subnet, warns on Security Profiles about any other, and refuses a new subnet, or a static WAN address, that overlaps.
