---
release-notes: false
---

- Changing the Security Profile of an Ethernet port or Wi-Fi password, or removing a profile's last Wi-Fi password, no longer deletes the reservations and published ports of the devices on it. The confirmation names the devices that keep them.
- A new reservation must be in the subnet of the profile the device is on and not reserved for another device. Reservations for profiles the device is not on are listed on its detail page as inactive, where they can be removed.
- A published port for a profile its device is not on shows as Inactive.
- An automatic port forward no longer stays open at a device's reserved address once the device takes an address on another profile.
