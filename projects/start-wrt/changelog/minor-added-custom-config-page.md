---
release-notes: true
---

- A page whose settings were changed in LuCI or by hand in a way StartWRT cannot show or keep now lists those changes, with their file and line and a link to LuCI, in place of its settings. Previously the page showed an error every few seconds, showed wrong values, or could overwrite the change on save. StartWRT also no longer turns off IPv6 at startup, or adds a second Admin profile, because of LAN settings it cannot read. A change StartWRT reads but does not manage, such as a second address on the LAN, is noted above the page's settings instead. An address written with a prefix length, a list of addresses, a static lease with several MAC addresses, and a firewall rule whose protocols are written as one option are now read.
