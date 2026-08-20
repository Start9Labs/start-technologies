# Settings

The Settings page contains system preferences, account management, and advanced tools. Navigate to `System > Settings`. The page is organized into tabs: General, Password, SSH Keys, Activity, Logs, Backup, and Advanced.

## General

### Updates

When a firmware update is available, a banner appears at the top of the General page showing the new version number. Expand the banner to view release notes before updating. See [Updating](updating.md) for the full update procedure.

### Preferences

- **Theme** — System, Dark, or Light. System follows your browser or OS preference.
- **Language** — A dropdown for the web interface language. Available languages are English, Spanish, German, French, and Polish (English is the fallback). The choice is saved per-router (server-side) — there is no automatic browser-language detection.
- **Timezone** — A searchable combo box populated from the device, listing hundreds of IANA time zones (e.g. labelled like "(GMT-6) America/Denver"). It's auto-detected from your browser during [initial setup](initial-setup.md) (falling back to UTC if detection fails). Changing the timezone restarts the schedule engine so that [WAN Blackout](security-profiles.md#wan-blackout) and [Wi-Fi Blackout](wifi-schedules.md) windows fire at the correct local time. It also affects activity timestamps and log timestamps.

Theme and Language changes are previewed immediately when you select them, and saved when you click "Save". If you leave the page without saving, the preview reverts to your saved settings.

### Remote Access

Controls whether the web interface (and SSH) accepts connections arriving on the WAN port — ports 80, 443, and 22. Devices on your local network can always reach the web interface at `router.lan`, whatever this is set to.

**To manage your router when you are away from home, use an [Inbound VPN](inbound-vpn.md)** — this works regardless of the Remote Access setting. VPN clients join your network as local devices (with the access granted by their [Security Profile](security-profiles.md)), so once connected you browse to the web interface exactly as you would at home. No port forwards or address whitelists on the router are needed. If your router sits behind another router, forward the VPN's WireGuard port on that upstream router; if your ISP uses [CGNAT](cgnat.md), inbound connections cannot reach your router at all — consider [StartTunnel](/start-tunnel/) as a gateway.

The options control direct WAN-side access only:

- **When behind NAT** (default) — WAN-side connections are accepted only from private source addresses (RFC 1918 for IPv4, ULA for IPv6), and only while the router's own WAN address is itself private. This is for routers that sit behind another router (e.g. an ISP box): devices on that upstream network can reach the web interface. From the public Internet, the web interface is never reachable in this mode — even if a public, globally routable address later appears on the WAN.
- **Never** — No WAN-side access at all, even from an upstream private network. The web interface is reachable only from the local network (including through an Inbound VPN).
- **Always** — WAN-side connections are accepted from any address. With a public IP, the web interface is reachable from anywhere on the Internet via the WAN IP or a [Dynamic DNS](ddns.md) domain.

> [!WARNING]
> Selecting "Always" exposes your router's admin interface to the public Internet. Only use this if you understand the security implications and have a strong admin password.

### Security

- **Download Root CA** — Download the router's Root CA certificate, saved as `startwrt-ca.crt`. See [Trusting Your Root CA](trust-ca.md) for installation instructions.

### About

The General page shows an About block with the firmware **Version** and a **Build** identifier (a short git hash; hover to see the full hash). These are useful when filing bug reports.

## Password

Change your admin password. The admin password protects the web interface and is separate from the Wi-Fi password.

1. Navigate to `System > Settings > Password`.

1. Enter your current password.

1. Enter and confirm your new password (minimum 12 characters).

1. Click "Save".

## SSH Keys

Manage the public keys authorized to access your router's command line over SSH. See [SSH Access](ssh.md) for adding keys and connecting.

## Activity

View a log of administrative actions taken through the web interface. Each entry shows:

- **Status icon** — Green check for successful actions, red X for failures.
- **Timestamp** — When the action occurred.
- **Summary** — A description of the action performed.
- **Error details** — If the action failed, the error message is shown below the summary.

Individual entries can be deleted, or click "Clear All" to remove the entire log. The list is paginated with 10 entries per page.

## Logs

View real-time system logs streamed from the router via WebSocket. Useful for diagnosing network issues, monitoring VPN connections, or verifying firewall behavior.

Navigate to `System > Settings > Logs` to open the live log viewer. You can download the full log as a text file or scroll to the bottom to follow new entries in real time.

## Backup

Download a backup of your router's configuration, or restore from a previously downloaded one. See [Backups](backups.md) for the full procedure.

## Advanced

The Advanced tab contains power-user tools:

- **Launch LuCI Interface** — Opens the underlying OpenWrt LuCI admin panel in a new tab for direct access to low-level configuration.
- **Download Support Diagnostics** — Generates and downloads a diagnostic bundle for troubleshooting with Start9 support.
- **Factory Reset** — Erases all settings (excluding the sticker Wi-Fi password) and reboots the router. See [Factory Reset](factory-reset.md) for details.

> [!WARNING]
> Factory reset is irreversible. Create a [backup](backups.md) first if you want to preserve your configuration.
