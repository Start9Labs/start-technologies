# Factory Reset

A factory reset restores StartWRT to its default state at unboxing. There are two ways to reset: from the web interface (soft reset) or from a microSD card (reflash).

## Soft Reset (Web Interface)

A soft reset erases the overlay filesystem where all configuration changes are stored, then reboots the router. The base firmware (read-only squashfs) is untouched — only your customizations are removed. The Wi-Fi password survives because it is re-read from the router's EEPROM on boot.

1. Navigate to `System > Settings`.

1. Click "Factory Reset".

1. Confirm the action.

The router will reboot. After reboot:

- Wi-Fi works immediately using the original sticker password (re-read from the router's EEPROM on boot).
- The admin password is cleared — you will be prompted to create a new one via the captive portal.
- All settings (security profiles, VPN configs, firewall rules, SSH keys, etc) are wiped.

> [!WARNING]
> A factory reset cannot be undone. Create a [backup](backups.md) first if you want to preserve your settings.

## Reflash (microSD)

A microSD reflash boots the router from a StartWRT image and replaces the firmware entirely. The router enters setup mode and brings up the `StartWRT` Wi-Fi network limited to a single client, with a captive portal that auto-opens the setup wizard.

1. Create a bootable microSD card — see [Installing StartWRT](installing.md).

1. Power off the router, insert the microSD card, and power it back on.

1. Connect to the `StartWRT` network via ethernet or by using the Wi-Fi password printed on the sticker. The captive portal opens the wizard automatically.

1. Choose a reflash path:

   - **Keep settings** — Keeps your settings, prompts for a new admin password, and replaces the firmware. Your configuration (including Wi-Fi and profile settings) is preserved. User-installed extra package binaries are wiped, so you will need to reinstall them — but their config files are retained. See also [Updating](updating.md).
   - **Fresh Start** — Wipes everything and installs a clean copy of StartWRT. You set a new admin password, and the timezone is auto-detected from your browser (you can change it later in [Settings](settings.md)). After reboot, Wi-Fi comes back up automatically using the sticker password re-read from the router's EEPROM — no Wi-Fi credentials are carried over from the old configuration. Equivalent to a factory reset plus a firmware reinstall.

1. When the wizard completes, power off the router, remove the microSD card, and power it back on.

> [!NOTE]
> On a DIY or unprogrammed board with no Wi-Fi password in the EEPROM, the wizard is reachable over Ethernet only, and the reflashed router boots with no Wi-Fi until you run `startwrt-cli set-wifi-password`. See [Installing StartWRT](installing.md#diy-and-unprogrammed-boards).

## What Gets Wiped

| Soft Reset | Keep settings (microSD) | Fresh Start (microSD) |
|------------|------------------|-----------------------|
| All settings and customizations | Settings preserved | All settings and customizations |
| Admin password cleared | New admin password | Admin password cleared |
| Firmware unchanged | Firmware replaced | Firmware replaced |
| Wi-Fi password preserved | Wi-Fi password preserved | Wi-Fi password preserved |

The Wi-Fi password survives in every case. For Soft Reset and Fresh Start — which wipe the overlay — it is re-read from the router's EEPROM on boot. With Keep settings, the existing Wi-Fi configuration (including any password you set yourself) is preserved as-is.

## Lost Wi-Fi Password

The Wi-Fi password is printed on the sticker on the bottom of your router and stored in the router's EEPROM. The EEPROM value is the password restored by a factory reset; during normal operation the active password is whatever is in the running configuration, so if you have replaced the **Default** entry with your own, that password is what's in effect. If you are still logged in, you can also reveal or copy it on the `Points of Entry > Wi-Fi > Passwords` page (the **Default** entry). On a DIY or unprogrammed board that has no EEPROM Wi-Fi password, set one via the GUI (if connected via ethernet) or with `startwrt-cli set-wifi-password`. See [Installing StartWRT](installing.md#diy-and-unprogrammed-boards) for details.
