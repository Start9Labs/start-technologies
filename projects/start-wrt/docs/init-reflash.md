# StartWRT Init & Reflash Proposal

## Context

StartWRT routers use **two separate passwords** set at different times:

| Password | Set by | When | Purpose |
|----------|--------|------|---------|
| **WiFi password** | Hardware vendor (during manufacture) | Programmed into EEPROM tag 0x2F | WPA2-PSK WiFi access |
| **Admin password** | End user | First GUI access after unboxing, factory reset, or reflash | Web UI authentication |

The WiFi password is a unique **12-character** string from a 67-char unambiguous charset, printed on a sticker on the bottom of the device. It provides **~72.3 bits of entropy**, making rainbow table attacks infeasible even with a static SSID.

The admin password is user-chosen (minimum 12 characters, any format) and stored in `/etc/shadow` on the overlay filesystem.

### Password lifecycle

| Stage | WiFi password | Admin password | Other settings |
|-------|--------------|----------------|----------------|
| **Manufacturing** | Vendor programs EEPROM tag 0x2F | Not set | Default config |
| **End user unboxing** | Read from EEPROM on first boot, written to UCI | User creates via captive portal | User configures |
| **Factory reset** (no microSD) | Re-read from EEPROM on reboot | Cleared — user sets via captive portal on reboot | Wiped (overlay gone), firmware unchanged |
| **Reflash / Update** (microSD) | Re-read from EEPROM after reflash → reboot | User sets in wizard before reboot | **Preserved**, firmware replaced |
| **Fresh Start** (microSD) | Re-read from EEPROM after reflash → reboot | User sets in wizard before reboot | Wiped, firmware replaced |

If EEPROM tag 0x2F is missing or invalid (DIY install on a board the vendor never programmed, or a corrupt blob), **the AP does not come up**. The operator must connect over ethernet/serial and use `startwrt-cli set-wifi-password` (see "Manual password provisioning" below) to provision one into UCI.

### Storage Architecture

The BPI-F3's persistent storage layers:

| Storage | Contents | Purpose |
|---------|----------|---------|
| **SPI NOR** (4MB) | U-Boot | Bootloader — selects eMMC or microSD boot |
| **I²C EEPROM** (24c02, 256 bytes, bus 2 / 0x50) | ONIE TLV blob (MAC, serial, **WiFi PMK at tag 0x2F**, CRC) | Vendor-programmed identity + WiFi password — survives reflash, factory reset, and overlay wipe |
| **eMMC — firmware partitions** | Kernel, SquashFS root, overlay | Main OS — factory reset wipes overlay only |
| **microSD** (removable) | Bootable reflash image | Recovery / reflash (Part 3) |

The EEPROM is read at runtime via the `at24` driver bound to the device tree (`/sys/bus/i2c/devices/2-0050/eeprom`). StartWRT only **reads** from the EEPROM — programming is a manufacturing responsibility. See `backend/ctrl/src/eeprom.rs` for the parser and resolution logic.

### Manual password provisioning (`startwrt-cli set-wifi-password`)

When EEPROM tag 0x2F is missing or invalid (DIY install, unprogrammed board), the daemon logs a warning at boot and brings up no AP. The operator runs `startwrt-cli set-wifi-password` (over ethernet or serial) to provision one:

- **Default mode** (`startwrt-cli set-wifi-password`) generates a random 12-character password from `PASSWORD_CHARS`, prints it to stdout, writes it to `/etc/config/wireless`, and reloads WiFi. The operator must record the printed password (it's the new sticker copy).
- **Manual mode** (`startwrt-cli set-wifi-password --manual`) prompts for a password with hidden input + confirmation. Same validation: exactly 12 ASCII characters from `PASSWORD_CHARS`.

**Persistence semantics:** the password set by this utility lives only in the overlay (`/etc/config/wireless`). Factory reset wipes the overlay → `restore_wifi_if_needed` re-reads EEPROM. If the EEPROM still has no tag 0x2F, the AP doesn't come up and the operator must run `set-wifi-password` again. EEPROM is canonical; the overlay is current state.

---

## Part 1: Manufacturing

### Vendor responsibilities

Manufacturing happens before the device leaves the vendor's facility. The vendor:

1. Generates a random 12-character password from `PASSWORD_CHARS` (the charset defined in `backend/ctrl/src/lib.rs`).
2. Programs the I²C EEPROM (bus 2, address 0x50) with an ONIE TLV blob containing tag 0x2F = the password (12 ASCII bytes).
3. Prints the same password on a sticker affixed to the device.
4. Flashes firmware to eMMC.

### StartWRT-side handling

`startwrt-ctrld` does not participate in EEPROM programming. On every boot (eMMC or microSD), the daemon's normal-mode startup calls `restore_wifi_if_needed`, which:

1. Returns early if `/etc/config/wireless` already has a key on the AP interface.
2. Reads EEPROM tag 0x2F, validating: ONIE magic, total_length, trailing CRC-32, tag presence, 12-byte length, every byte in `PASSWORD_CHARS`.
3. On success: writes the password to UCI, runs `wifi reload`, and brings up SSID `StartWRT`.
4. On any failure (including unprogrammed EEPROM): logs a warning and leaves the AP unconfigured. Recovery is via `startwrt-cli set-wifi-password` (above).

This means a vendor-programmed board "just works" out of the box — no on-device init step is required after firmware flashes.

### Password Character Set

| Category | Characters | Count |
|----------|-----------|-------|
| Uppercase | `A-Z` minus `I`, `O` | 24 |
| Lowercase | `a-z` minus `i`, `l`, `o` | 23 |
| Digits | `2-9` | 8 |
| Special | `! @ # $ % ^ & * - _ + =` | 12 |
| **Total** | | **67** |

12 chars × 67-char set ≈ **72.3 bits entropy**

The same charset is enforced by:
- `backend/ctrl/src/eeprom.rs::read_wifi_password` (rejects non-charset bytes from EEPROM)
- `backend/ctrl/src/init.rs::validate_password` (validates manual entry in `set-wifi-password --manual`)

---

## Captive Portal Mechanism

Both first-time setup (Part 2) and reflash (Part 3) use the same two-layer mechanism:

1. **DNS hijacking** — `dnsmasq` configured with `address=/#/<router IP>` makes all DNS queries resolve to the router.
2. **Captive portal detection** — Modern OSes probe known URLs on network join. When DNS hijacking redirects these probes, the OS detects a captive portal and auto-opens a browser pointed at the router's setup page.

Both scenarios use the same `StartWRT` SSID and the same EEPROM-derived WiFi password. They differ only in what the setup page does.

---

## Part 2: First-Time User Setup (Unboxing / Factory Reset)

After unboxing or factory reset, WiFi works immediately but no admin password is set. A captive portal on the main network forces setup before normal use.

```
1. User powers on router (or reboots after factory reset)
2. Daemon reads EEPROM → writes WiFi password to UCI → AP "StartWRT" comes up
3. User connects to WiFi "StartWRT" using sticker password
4. No admin password → captive portal active (all DNS queries → router IP)
5. OS detects captive portal → auto-opens browser with setup page
6. GUI prompts: "Create your admin password"
7. User creates password (minimum 12 characters)
8. User confirms password
9. SHA-512 crypt → /etc/shadow
10. DNS hijacking disabled → normal browsing resumes
11. User is logged in
```

The captive portal ensures the admin password cannot be ignored — all internet access is blocked until setup is complete. If the user dismisses the captive portal popup, native apps and browsing remain broken until they open a browser and reach the setup page.

Implementation: `dnsmasq` `address=/#/<router IP>` when no root hash in `/etc/shadow`. Removed after password is set. Unlike the reflash captive portal, `max_num_sta` is not restricted — the device is already on a private network protected by the sticker password.

> **Note**: After a reflash, the admin password is set in the captive portal wizard (Part 3), so this flow only applies to unboxing and factory reset.

---

## Part 3: Reflash / Reset Flow

### Trigger

Boot from **microSD card** with StartWRT image. U-Boot prioritizes microSD.

### Setup-mode AP

In setup mode, `startwrt-ctrld` reads EEPROM tag 0x2F at startup and brings up the same `StartWRT` SSID used for normal operation, with `max_num_sta=1` to limit the wizard to a single client. If tag 0x2F is missing, the daemon logs a warning, no AP comes up, and the wizard remains reachable over ethernet only.

| Setting | Value |
|---------|-------|
| **SSID** | `StartWRT` |
| **Password** | EEPROM tag 0x2F (sticker password) |
| **Security** | WPA2-PSK |
| `max_num_sta` | `1` (single-client limit during setup) |
| **DNS** | All queries resolve to router IP |

The user connects to `StartWRT` using the sticker password. The captive portal redirects all requests to the wizard — the OS detects this and auto-opens a browser window.

### Detection Logic

| EEPROM tag 0x2F | eMMC has firmware | Wizard offers |
|:---:|:---:|---|
| Yes | Yes | **Update** or **Fresh Start** |
| Yes | No | **Fresh Start** only |
| No | Yes | Wizard reachable over ethernet only; offers **Update** or **Fresh Start** |
| No | No | Wizard reachable over ethernet only; offers **Fresh Start** |

In the "No EEPROM password" rows, the post-reflash device boots without WiFi. The end user must run `startwrt-cli set-wifi-password` to recover.

### Path A: Update (keep settings, new admin password)

Physical access (microSD) = sufficient authorization.

```
 1. Detect onboard disk with config + firmware
 2. User selects "Update"
 3. Create admin password (minimum 12 characters)
 4. Confirm admin password
 5. Backup config files (sysupgrade conffiles list, excluding /etc/shadow)
 6. Flash new firmware (replace squashfs base, wipe overlay)
 7. Restore config files
 8. Write admin hash → /etc/shadow on new overlay
 9. Mark overlay FS_STATE_READY so first boot doesn't wipe it
10. "Update complete. Remove microSD and reboot."
```

On first boot from eMMC: daemon reads EEPROM → writes WiFi password to UCI → AP comes up. Admin login works immediately. Config files (firewall rules, WiFi profiles, SSH keys, etc.) are preserved via `sysupgrade` conffiles. User-installed package binaries are wiped — users must reinstall packages, though their config files are retained.

`/etc/config/wireless` is preserved across Update so that user-customized VLAN/profile state survives. `restore_wifi_if_needed` short-circuits when the AP interface already has a key, so the EEPROM fallback does not overwrite the restored config; it only kicks in if the restored wireless config is missing or incomplete.

### Path B: Fresh Start (full wipe, new admin password)

```
 1. User selects "Fresh Start"
 2. Select Language, Country, Drive/Partition
 3. Create admin password (minimum 12 characters)
 4. Confirm admin password
 5. Wipe onboard disk config overlay entirely
 6. Flash fresh firmware
 7. Write admin hash → /etc/shadow on new overlay
 8. Mark overlay FS_STATE_READY
 9. "Setup complete. Remove microSD and reboot."
```

On first boot from eMMC: daemon reads EEPROM → writes WiFi password to UCI → AP comes up. Admin login works immediately. All settings start fresh.

### Package Management

All packages required by StartWRT are included in the firmware image. Both Update and Fresh Start wipe the overlay, so user-installed package binaries are always removed to avoid conflicts with the new firmware or UCI config files. The Update path preserves config files via `sysupgrade` conffiles — so package configs survive even though binaries are wiped. Users will need to reinstall any additional packages after reflash.

### Web UI Architecture

| Component | Details |
|-----------|---------|
| **Frontend** | Angular app from microSD image |
| **Backend** | `startwrt-ctrld` in "setup mode" |
| **Access** | Captive portal (DNS hijack → any URL reaches wizard) |

Setup mode is detected by `setup::is_setup_mode`: if the boot device is *not* an eMMC, the daemon enters setup mode and exposes only the `setup::status` and `/api/setup/flash` endpoints (the latter consumed by the wizard).

### Boot Detection

```
microSD boot (setup mode)?
├── Yes → Read EEPROM tag 0x2F
│         ├── Found → Start "StartWRT" AP (max_num_sta=1) → captive portal → wizard
│         └── Missing → Skip AP setup; wizard reachable over ethernet only
└── No  → Normal boot
          1. WiFi key present in /etc/config/wireless?
          │   ├── Yes → skip restore
          │   └── No  → Read EEPROM tag 0x2F
          │             ├── Found → Write to UCI, reload WiFi
          │             └── Missing → Log warning, no AP. Recover via
          │                            startwrt-cli set-wifi-password
          2. Admin password set?
              ├── Yes → Normal operation
              └── No  → Captive portal active (forces admin setup before normal browsing)
```

---

## Design Notes

### WiFi PSK and Security Profiles

Passphrase as `iface.key` is fully compatible with identity PSK. The code in `wifi.rs` treats the key as an opaque string (read/write pass-through). Hostapd derives the PMK internally from the passphrase. Per-profile PSKs in `wpa_psk_file` are processed independently.

```
WiFi Interface (iface.key = sticker password)
├── dynamic_vlan = ALLOWED
├── WifiStation { key: "profile1_pass", vid: 101 }
└── Default: sticker password → no VLAN → main LAN
```

### Why Plaintext, Not PMK

The sticker password is physically printed on the device — storing it as plaintext in EEPROM is no less secure than the sticker itself. Hostapd derives the PMK internally from the passphrase, so there is no need for the backend to perform PBKDF2-SHA1 derivation. Storing plaintext simplifies the codebase (no `pbkdf2`/`sha1`/`hmac` dependencies) and makes the stored value directly usable as a WPA2 passphrase without conversion.

### WiFi Key in UI

The WiFi passphrase is never displayed in the admin interface. The sticker is the source of truth. Users who lose the sticker can read tag 0x2F over ssh (`hexdump /sys/bus/i2c/devices/2-0050/eeprom`) or replace it with a self-chosen password via `startwrt-cli set-wifi-password`. The latter only updates UCI — EEPROM remains the canonical fallback after factory reset.

---

## Implementation

### Files

| File | Purpose |
|------|---------|
| `backend/ctrl/src/eeprom.rs` | ONIE TLV parser, CRC-32 verification, tag 0x2F reader |
| `backend/ctrl/src/init.rs` | `set_wifi_password` (default + manual modes), `configure_wifi`, `restore_wifi_if_needed` |
| `backend/ctrl/src/setup.rs` | Reflash mode: disk detection, conffiles backup/restore, overlayfs management |
| `backend/ctrl/src/flash.rs` | Raw eMMC flash from microSD, partition manipulation |
| `backend/ctrl/src/bins/cli.rs` | CLI dispatch — exposes `set-wifi-password`, `flash`, `verify` |
| `backend/ctrl/src/bins/daemon.rs` | Boot-time WiFi auto-restore, setup-mode detection, flash HTTP handler |
| `build/stage-files.sh` | Serial dispatcher (prints setup-mode hint, drops to login — no longer calls into startwrt-cli) |
| `genkey.py` | Vendor-side password generator (matches `PASSWORD_CHARS`) |
| `web/` (setup wizard) | Angular wizard UI |

### Reusable Code

| Source | Reuse |
|--------|-------|
| `auth.rs` — `rpassword::prompt_password()` | No-echo prompting in `set-wifi-password --manual` |
| `start-os::util::io::AtomicFile` | Atomic write pattern for `/etc/shadow`, conffiles |
| `uciedit` crate | UCI config writing |
| `wifi.rs` | WiFi PSK + dynamic VLAN structure |
| `start-os/web/projects/setup-wizard/` | Angular wizard reference |

### New Dependencies

None — plaintext password storage in EEPROM eliminates the need for `pbkdf2`, `hmac`, `sha1`. ONIE TLV parsing and CRC-32 are implemented in-tree (`eeprom.rs`).

---

## Verification

| # | Test | Verify |
|---|------|--------|
| 1 | Charset validation | 12-char valid passes; ambiguous/wrong-length rejected (`init::validate_password`, `eeprom::read_wifi_password`) |
| 2 | TLV parsing | Vendor blob parses; bad magic, bad CRC, truncated headers rejected (`eeprom::tlv::tests`) |
| 3 | EEPROM read on hardware | `read_wifi_password()` returns the sticker password on a vendor-programmed board |
| 4 | Unprogrammed EEPROM | All-0xFF blob → `Ok(None)`; daemon logs warning, no AP comes up |
| 5 | WiFi connectivity | Passphrase from EEPROM works with hostapd WPA2-PSK |
| 6 | First-time admin | GUI prompts for admin password when unset |
| 7 | Factory reset | Overlay wiped → daemon re-reads EEPROM → WiFi restored, admin unset, captive portal active |
| 8 | Update path | Settings preserved (including `/etc/config/wireless`), `/etc/shadow` rewritten, admin re-prompted only if not set in wizard |
| 9 | Fresh Start | Everything wiped, WiFi restored from EEPROM on reboot, admin re-prompted only if not set in wizard |
| 10 | Identity PSK | After reset, sticker WiFi works; profiles can be recreated |
| 11 | `set-wifi-password` (default) | Generates 12-char password, prints to stdout, AP comes up with that key |
| 12 | `set-wifi-password --manual` | Hidden prompt + confirmation; rejects bad length / non-charset entries |
| 13 | Recovery from missing EEPROM | `set-wifi-password` brings AP up; factory reset clears overlay → AP gone again until utility re-run |
