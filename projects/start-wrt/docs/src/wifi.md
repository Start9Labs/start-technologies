# Wi-Fi

StartWRT uses a single Wi-Fi network (one SSID) with multiple passwords. Each password maps to a different [Security Profile](security-profiles.md). The password a device uses to connect determines its profile — no manual network selection or VLAN configuration required.

## How It Works

Traditional routers create separate Wi-Fi networks (separate SSIDs) for different access levels. StartWRT takes a different approach: one SSID with multiple WPA2 passwords. When a device connects, the router identifies which password was used and assigns the corresponding Security Profile. This is powered by WPA2's identity PSK feature with dynamic VLAN assignment.

From the user's perspective, everyone connects to the same network name — the password is what determines their access level.

## The Default Password

The Wi-Fi password printed on the sticker on the bottom of your router is the default password. It maps to the default Security Profile (typically "Admin" with full access). This password is set during manufacturing and stored in the router's EEPROM. The EEPROM value is only authoritative on a [factory reset](factory-reset.md), when it is re-read to restore the default password. During normal operation the active password is whatever is in the router's running configuration: you can delete the **Default** entry and create a new one, and that new password is respected regardless of the EEPROM value. It also appears on the `Points of Entry > Wi-Fi > Passwords` page as the **Default** entry (mapped to the Admin profile), where you can reveal or copy it.

> [!NOTE]
> Keep the sticker password safe. On a DIY or unprogrammed board with no EEPROM Wi-Fi password, set one in the GUI (if connected via ethernet) or with `startwrt-cli set-wifi-password`. See [Installing StartWRT](installing.md#diy-and-unprogrammed-boards).

## Adding a Wi-Fi Password

1. Navigate to `Points of Entry > Wi-Fi > Passwords`.

1. Click "Add".

1. Configure the password:

   - **Label** — A descriptive name for this password (e.g. "Home", "Guest Network", "Kids").
   - **Password** — Enter a password (8–63 characters) or click "Generate" to create a strong random password.
   - **Security Profile** — Select the [Security Profile](security-profiles.md) this password should map to.

1. Click "Save".

Share this password with the people or devices that should receive that profile. They connect to the same network — the router handles the rest.

## Removing a Wi-Fi Password

1. Navigate to `Points of Entry > Wi-Fi > Passwords`.

1. Select the password from the actions menu and click "Delete".

> [!WARNING]
> Removing a Wi-Fi password immediately disconnects all devices using it. Those devices will need a different password to reconnect.

## Settings

Configure the Wi-Fi radio hardware under `Points of Entry > Wi-Fi > Settings`:

- **Enable Wi-Fi** — Global toggle to turn the wireless radio on or off. When disabled, no devices can connect via Wi-Fi.

- **SSID** — The network name that devices see when scanning for Wi-Fi (default: `StartWRT`). All passwords share this single SSID.

- **Broadcast** — Toggle SSID visibility. When off, the network is hidden from device scans and users must manually enter the network name to connect.

- **Frequency Band** — 2.4 GHz, 5 GHz, or Both. 2.4 GHz has better range and wall penetration. 5 GHz offers higher speeds but shorter range. Both enables dual-band operation.

- **Broadcast Separately** — Shown only when Band is "Both". When enabled, the 5 GHz band gets a separate SSID with a `-5G` suffix (e.g. `StartWRT` and `StartWRT-5G`). Useful if you want to control which band a device connects to.

- **Channel** — Separate dropdowns for each band. **Auto** (recommended) lets the router select the least congested channel. You can also select a specific channel: 1–11 for 2.4 GHz, or 36–165 for 5 GHz.

> [!WARNING]
> Changing the SSID disconnects all Wi-Fi clients. You will be prompted to confirm before the change is applied.

> [!TIP]
> For 2.4 GHz, channels 1, 6, and 11 are the only non-overlapping channels. If you experience interference, try one of these.

## Example

| Label | Password | Profile | Who uses it |
|-------|----------|---------|-------------|
| Default | *(sticker)* | Admin | You — full LAN and Internet access |
| Guest | `Rv3kWpTm8xNqYb5J` | Guest | Visitors — Internet only, through Proton VPN |
| Kids | `Dn7cXfHs4uEgAw2R` | Children | Your children — Internet during daytime only, with DNS filtering |
| IoT | `Ym9pVtKe6jQrZh3F` | Smart Devices | IoT devices — limited Internet, no LAN access |
