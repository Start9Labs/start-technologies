# Settings

The Settings page contains system preferences, account management, and advanced tools. Navigate to `System > Settings`. The page is organized into tabs: General, Password, SSH Keys, Activity, Logs, Backup, and Advanced.

## General

### Updates

When a firmware update is available, a "vX.Y.Z released!" panel appears at the top of the General page. Expand it to view release notes before updating. See [Updating](updating.md) for the full update procedure.

### Preferences

- **Theme** — System, Dark, or Light. System follows your browser or OS preference.
- **Language** — A dropdown for the web interface language. Available languages are English, Spanish, German, French, and Polish (English is the fallback). The choice is saved per-router (server-side) — there is no automatic browser-language detection.
- **Timezone** — A searchable combo box populated from the device, listing hundreds of IANA time zones (e.g. labelled like "(GMT-6) America/Denver"). It's auto-detected from your browser during [initial setup](initial-setup.md) (falling back to UTC if detection fails). Changing the timezone restarts the schedule engine so that [WAN Blackout](security-profiles.md#wan-blackout) and [Wi-Fi Blackout](wifi-schedules.md) windows fire at the correct local time. It also sets the time zone of log timestamps. Activity timestamps are shown in your browser's time zone.

Theme and Language changes are previewed immediately when you select them, and saved when you click "Save". If you leave the page without saving, the preview reverts to your saved settings.

### Security

- **Download Root CA** — Download the router's Root CA: a configuration profile on iPhone and iPad, `startwrt-ca.crt` elsewhere. See [Trusting Your Root CA](trust-ca.md) for installation instructions.

### About

The General page shows an About block with the firmware **Version** and a **Build** identifier — the first 12 characters of the firmware's git hash (hover to see the full hash). These are useful when filing bug reports.

## Password

Change your admin password. The admin password protects the web interface and is separate from the Wi-Fi password.

1. Navigate to `System > Settings > Password`.

1. Enter your current password.

1. Enter and confirm your new password (minimum 12 characters).

1. Click "Save".

Saving signs out every other browser and `startwrt-cli` login. The browser you saved from stays signed in.

## SSH Keys

Manage the public keys authorized to access your router's command line over SSH. See [SSH Access](ssh.md) for adding keys and connecting.

## Activity

View a log of administrative actions taken through the web interface, along with sign-ins, SSH sessions, and each time the router starts. Each entry shows:

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

- **Launch LuCI Interface** — Opens the underlying OpenWrt LuCI admin panel in a new tab for direct access to low-level configuration. If a change made in LuCI or by hand is one StartWRT cannot show or keep, each page it affects lists the change, with a link to LuCI, in place of its settings. A change StartWRT reads but does not manage, such as a second address on the LAN, is noted above the page's settings.
- **Download Support Diagnostics** — Generates and downloads a diagnostic bundle for troubleshooting with Start9 support. Passwords and keys are removed, and MAC addresses, public IP addresses, names and domains are replaced with placeholders.
- **Factory Reset** — Erases all settings (excluding the sticker Wi-Fi password) and reboots the router. See [Factory Reset](factory-reset.md) for details.

> [!WARNING]
> Factory reset is irreversible. Create a [backup](backups.md) first if you want to preserve your configuration.
