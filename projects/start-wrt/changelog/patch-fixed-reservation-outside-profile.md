---
user-facing: true
---

- An IPv4 reservation must now be in the subnet of the device's security profile; the router used to accept an address in another profile's subnet, which DHCP silently ignored.
- A device whose reservation no longer matches its profile, such as after it moves to another profile, shows its actual address with a warning on its detail page, and its published ports report the mismatch.
