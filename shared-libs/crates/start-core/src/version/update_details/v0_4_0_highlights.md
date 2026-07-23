# Welcome to stable StartOS 0.4.0

StartOS 0.4.0 has left early access. You updated from a pre-release, so here is what's new since the previous beta:

## Backups — please read

Backups now use a new **v2** format, written to a `StartOSBackupsV2` folder on your
backup target. It is faster and more space-efficient, and the backup report now shows
how long each service took. Your existing `StartOSBackups` (v1) backups are left in
place until you remove them.

**What to do:**

1. **Create a fresh backup after updating.** Your first v2 backup to a target that
   still holds a v1 backup needs room for both, so StartOS asks you to confirm the
   format change — and refuses if the target does not have enough free space.
2. **Then reclaim the space.** Once a v2 backup exists, use **Delete old backup** on
   the backup page (or follow the reminder notification) to remove the old v1 folder.
   This never touches your v2 backups. Removal is instant; the space itself is
   reclaimed in the background, with a notification when it finishes.

Services can also stream their own backup sub-progress now, so backups report
detailed, phased progress like updates and installs.

## Sign-in — please read

Signing in now enrolls a **per-device key**, and every request your browser makes
is signed with that key — session cookies are gone entirely.

**What to do:**

1. **Sign in again on each device.** This update signs out everything that was
   signed in.
2. **Use a current browser.** Signing in requires Ed25519 WebCrypto support — any
   evergreen browser qualifies (Safari 17, Firefox 130, Chrome/Edge 137, or newer).

**System → Active Sessions** manages enrolled devices the way it managed sessions,
and devices idle for 30 days are removed automatically.

**Changing your master password no longer asks for the current one** — being signed
in is enough. That also means a forgotten password is no longer fatal: any device
that is still signed in can set a new one.

## Networking

- **Automatic gateway setup.** StartOS now opens its own public ports and publishes
  private-domain DNS by talking to your gateway — a home router (via PCP, NAT-PMP, or
  UPnP) or a StartTunnel — instead of leaving it to you as a manual step.
- **Private domains over StartTunnel.** Private domains now resolve for StartTunnel
  clients, so every gateway, not just routers, can serve them.
- **`.local` over StartTunnel.** Your server's `<hostname>.local` name now resolves
  for clients connected through a WireGuard gateway like StartTunnel, not just on
  your LAN. (A gateway with DNS injection disabled still needs a manual record.)
- **Public domains on IPv4 + IPv6.** A public domain is now served over both IPv4
  and IPv6 whenever its gateway carries a global IPv6 address, and domain setup
  checks and displays both records.
- **IPv6 exposure control.** A service's global IPv6 address can now be kept
  **Local** (reachable from your own network only — the default) or made **Public**
  (reachable from the Internet, with the gateway port opened automatically).

## Interface & marketplace

- **Service Interfaces tab.** Service interfaces move to their own sidebar tab.
- **Redesigned marketplace.** The marketplace has been completely redesigned.
- **Logs moved to System.** OS Logs and Kernel Logs now live at the bottom of the
  System menu; the top-level Logs tab is gone.

## Reliability & fixes

- **Installing large services no longer stalls or freezes the box.** Package
  downloads and installs now pace their disk writes, eliminating the multi-minute
  hangs some boxes hit on big packages.
- **A network blip can no longer take down DNS box-wide.** Container DNS recovers
  the moment upstream resolvers change, instead of failing for every service until
  the old upstreams came back or the box was rebooted.
- **A cancelled or failed service update rolls back cleanly.** The service's data
  is restored to its pre-update state and the previous version restarts, instead of
  the service getting stuck on partially-migrated data.
- **The installer refuses a "Preserve" selection it cannot honor.** Choosing drives
  that cannot keep your existing data now fails up front with guidance, instead of
  silently erasing the data drive.
- **The UI no longer freezes after you enter your master password** when creating or
  restoring a backup or changing your password — on phones this could look like a
  20-second hang.
- **Graceful shutdown on power events.** UPS- and host-initiated shutdowns now tear
  services down cleanly before power-off.
- **iOS Root CA install** via a configuration profile, fixing certificate setup on
  recent iOS.
- **Non-ASCII Wi-Fi SSIDs** — accented letters and typographic apostrophes are now
  accepted.

## Security

- **From cookies to signatures.** API authentication has moved from session cookies
  to per-device signing keys. Each request is signed with a key that never leaves
  your device, and each signature is tied to this server, so it is not valid
  anywhere else. See **Sign-in** above for what to do when updating.
- **Strict Content-Security-Policy.** The web UI, setup wizard, and diagnostic pages
  now forbid scripts and network connections to anything but the server itself.

For the complete list of changes, see [the full changelog on GitHub](https://github.com/Start9Labs/start-technologies/blob/master/projects/start-os/CHANGELOG.md).
