# Settings

The Settings page contains system preferences, account management, and advanced tools. Navigate to `System > Settings`. The page is organized into tabs: General, Password, SSH Keys, Backup, Logs, Activity, and Advanced. See also [SSH Access](ssh.md) and [Backups](backups.md) for dedicated documentation on those tabs.

## General

### Preferences

- **Theme** — System, Dark, or Light. System follows your browser or OS preference.
- **Language** — A dropdown for the web interface language. Available languages are English, Spanish, German, French, and Polish (English is the fallback). The choice is saved per-router (server-side) and only takes effect when you click "Save" — there is no automatic browser-language detection.
- **Timezone** — A searchable combo box populated from the device, listing hundreds of IANA time zones (e.g. labelled like "(GMT-6) America/Denver"). It's auto-detected from your browser during [initial setup](initial-setup.md) (falling back to UTC if detection fails). Changing the timezone restarts the schedule engine so that [WAN Blackout](security-profiles.md#wan-blackout) and [Wi-Fi Blackout](wifi-schedules.md) windows fire at the correct local time. It also affects activity timestamps and log timestamps.

### About

The General page shows an About block with the firmware **Version** and a **Build** identifier (a short git hash; hover to see the full hash). These are useful when filing bug reports.

### Remote Access

Controls whether the web interface is accessible from outside the local network. When remote access is enabled, the router detects whether it has a public or private WAN IP and adjusts accordingly. Remote access works over the router's WAN IP or [Dynamic DNS](ddns.md) domain, and is also reachable through an [Inbound VPN](inbound-vpn.md) connection.

- **When behind NAT** (default) — The recommended setting for most users. The admin-interface access rules (ports 80/443/22) are scoped to private and ULA source ranges (RFC 1918 for IPv4, `fc00::/7` for IPv6), so the admin interface stays reachable from your local network but closed to the public Internet — even if a public, globally routable address later appears on the WAN.
- **Never** — Disables remote access entirely. The admin interface is only accessible from devices on the local network.
- **Always** — Enables remote access at all times, even with a public IP.

> [!WARNING]
> Selecting "Always" exposes your router's admin interface to the Internet. Only use this if you understand the security implications and have a strong admin password.

### Security

- **Download Root CA** — Download the router's Root CA certificate, saved as `startwrt-ca.crt`. See [Trusting Your Root CA](trust-ca.md) for installation instructions.

### Updates

When a firmware update is available, a banner appears at the top of the General page showing the new version number. Expand the banner to view release notes before updating. See [Updating](updating.md) for the full update procedure.

## Password

Change your admin password. The admin password protects the web interface and is separate from the Wi-Fi password.

1. Navigate to `System > Settings > Password`.

1. Enter your current password.

1. Enter and confirm your new password (minimum 12 characters).

1. Click "Save".

## Logs

View real-time system logs streamed from the router via WebSocket. Useful for diagnosing network issues, monitoring VPN connections, or verifying firewall behavior.

Navigate to `System > Settings > Logs` to open the live log viewer. You can download the full log as a text file or scroll to the bottom to follow new entries in real time.

## Activity

View a log of administrative actions taken through the web interface. Each entry shows:

- **Status icon** — Green check for successful actions, red X for failures.
- **Timestamp** — When the action occurred.
- **Summary** — A description of the action performed.
- **Error details** — If the action failed, the error message is shown below the summary.

Individual entries can be deleted, or click "Clear All" to remove the entire log. The list is paginated with 10 entries per page.

## Advanced

The Advanced tab contains power-user tools:

- **Launch LuCI Interface** — Opens the underlying OpenWrt LuCI admin panel in a new tab for direct access to low-level configuration.
- **Download Support Diagnostics** — Generates and downloads a diagnostic bundle for troubleshooting with Start9 support.
- **Factory Reset** — Erases all settings (excluding the sticker Wi-Fi password) and reboots the router. See [Factory Reset](factory-reset.md) for details.

> [!WARNING]
> Factory reset is irreversible. Create a [backup](backups.md) first if you want to preserve your configuration.
