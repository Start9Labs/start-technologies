# Updating

StartWRT never updates automatically — updating always requires explicit action. There are two ways to update: an in-app update from the web interface (recommended), or a microSD reflash (a fallback if an in-app update ever fails). We highly recommend keeping StartWRT up to date for the latest security and performance patches, as well as to take advantage of new features.

## In-App Update (Recommended)

1. Navigate to `System > Settings > General`.

1. When a newer signed release is available, a "vX.Y.Z released!" accordion appears. Expand it to read the release notes.

1. Click "Update now" and confirm.

1. The download and apply progress is shown live. When it finishes, the router reboots. Once it is back online, the page reloads to the new interface automatically and takes you to the login page — updating signs everyone out, so simply log back in with your admin password. After you log in, an "Updated to vX.Y.Z" confirmation appears.

   > [!WARNING]
   > Do not unplug your router during the update or reboot. The update can take several minutes to apply. All network traffic — Wi-Fi, Ethernet, VPN connections, and port forwarding — will be interrupted until the router finishes restarting.

Firmware integrity is enforced cryptographically (a Blake3 commitment plus ed25519 release signatures), so only properly signed StartWRT releases will install. A tampered or unsigned image is rejected.

## Pages Open During an Update

Only the page that started the update reloads itself. If the interface was open anywhere else while the update was applied — another browser tab, another device — that page will show a "Refresh Needed" dialog once it notices the router is running newer firmware. Click **Reload** to load the new interface; nothing is reloaded out from under you automatically.

Updates also keep the router's low-level boot firmware (the eMMC boot partitions) in sync with the release — verified during the update and again on every boot — so every part of the boot chain ships and updates together. This is automatic; if it is already current, nothing is rewritten.

## Update by Reflashing (Fallback)

If an in-app update ever fails, you can update StartWRT by reflashing from a microSD card. Use the **Keep settings** path in the reflash wizard, which replaces the firmware while preserving your settings. See [Installing StartWRT](installing.md) for how to create a bootable microSD card, and [Factory Reset](factory-reset.md#reflash-microsd) for a walkthrough of the reflash wizard.
