# Physical Device Pre-Ship Test Plan

Manual QA checklist for flows that cannot be covered by OTA updates. Run on the exact ISO + hardware combo that will ship.

## Prerequisites

Hardware:

- [ ] Target device (bare, no OS installed) + power supply
- [ ] USB stick ≥ 8 GB (will be wiped) for install media
- [ ] Monitor with HDMI, USB keyboard, USB mouse
- [ ] Ethernet cable + router with DHCP + internet
- [ ] Laptop on same LAN with Firefox, a terminal, and an SSH client
- [ ] **Second StartOS device** running a the same version, but updated via OTA, with at least one service installed and running. Used by both §4 (restore) and §5 (transfer).
- [ ] External USB drive holding an encrypted backup taken from the second device (used in §4)
- [ ] The second device's **data drive** physically removable and reattachable to the target (used in §4). Set up this device with an external USB hard drive for ease of testing.

Artifacts:

- [ ] StartOS ISO for this release (e.g. `startos-0.4.0-beta.4-abcdef0_x86_64.iso`) on the laptop
- [ ] SHA256 of the ISO from the release manifest

---

## 1. Flash install media & install from USB

### 1.1 Verify and flash the ISO (on laptop)

Plug in the USB stick. Identify its device node:

```bash
lsblk -o NAME,SIZE,TRAN,MODEL
```

Confirm the stick's device node (e.g. `/dev/sdX`). **Double-check it is not the laptop's internal disk.**

Verify and flash:

```bash
sha256sum startos-*.iso                                    # must match release manifest
sudo umount /dev/sdX* 2>/dev/null || true
sudo dd if=startos-*.iso of=/dev/sdX bs=4M status=progress conv=fsync
sync
```

- [ ] sha256sum matches
- [ ] `dd` completes with no I/O error
- [ ] `sudo parted /dev/sdX print` on the flashed stick shows a hybrid ISO layout (one ISO9660 partition, one EFI partition)

### 1.2 Boot the device from USB

1. Plug the flashed USB, HDMI monitor, keyboard, mouse, and Ethernet into the bare device.
2. Power on and enter the firmware boot menu (F12 / F11 / ESC per SKU). Select the USB stick.

- [ ] Firmware boot menu lists the USB stick as a bootable entry
- [ ] GRUB/isolinux splash shows the StartOS boot menu with the release version string
- [ ] Default entry boots within 10 seconds without manual selection
- [ ] The live environment reaches a login prompt / installer console with no kernel panic
- [ ] No red `[FAILED]` lines in the boot scroll (transient `systemd-networkd-wait-online` warning is OK)

### 1.3 Run the on-device installer

From the live environment, follow the installer instructions shown on-screen to install StartOS to the device's internal disk.

- [ ] The installer lists the internal disk with correct vendor/model/capacity
- [ ] The USB install stick is **not** offered as an install target
- [ ] The install completes with no error messages
- DO NOT select Continue to Setup, simply press the power button and wait for it to shut down
- [ ] Remove the USB stick, then power the device back on

### 1.4 First cold boot from internal disk

- [ ] Device POSTs and boots from internal media with no USB attached
- [ ] **On UEFI + Secure Boot SKUs only:** a blue MokManager screen appears. Select `Enroll MOK` → `Continue` → `Yes` → enter the one-time password shown by the installer → `Reboot`. After reboot, MokManager does **not** reappear.
- [ ] **On SKUs with Secure Boot disabled:** no MokManager screen appears
- [ ] Device reaches a steady state: HDMI shows kiosk browser

---

## 2. Fresh setup wizard

In the laptop browser at `http://start.local`.

### 2.1 Language sanity

1. On the language page, select **Español**.

- [ ] All visible buttons and labels switch to Spanish immediately (e.g. "Siguiente" instead of "Next")

2. Switch back to **English (US)**.

### 2.2 Drive selection

- [ ] The internal disk of the device appears with correct capacity (±5%)
- [ ] No phantom disks appear
- [ ] Select the internal disk and continue

### 2.3 Mode: Start Fresh

- [ ] Enter password `QaTestPass!1`, confirm, and submit
- [ ] Enter server name `qa-test` and submit
- [ ] Success page shows "Setup complete"

### 2.4 Post-setup reachability

From the laptop:

```bash
ping -c 3 qa-test.local
ssh start9@qa-test.local                         # password: QaTestPass!1
```

- [ ] `ping qa-test.local` succeeds via mDNS (3/3 replies)
- [ ] SSH login succeeds with the configured password
- [ ] Selecting "Continue to Login" navigates to `http://qa-test.local` and shows the StartOS admin login page
- [ ] Logging in with `QaTestPass!1` reaches the admin dashboard
- [ ] The dashboard "System" page shows the expected version string matching the ISO under test

---

## 3. Hardware detection spot-check

On the device via SSH:

```bash
lsblk -o NAME,SIZE,MODEL,TRAN                    # storage
ip -br link                                      # NICs
ip -br addr                                      # IPs
ip route | grep default                          # gateway
lspci -nn | grep -Ei 'network|ethernet|wireless' # PCI NICs
```

- [ ] Every physical disk the SKU ships with is listed in `lsblk` with correct size
- [ ] Every physical NIC on the SKU shows `UP` in `ip -br link`
- [ ] `ip -br addr` shows a DHCP-assigned LAN address on the Ethernet interface
- [ ] `ip route` shows exactly one default route through the expected interface
- [ ] On Wi-Fi SKUs: `lspci` lists the Wi-Fi controller

---

## 4. Restore from backup

This is a **second full install cycle**. You are validating the restore path that only exists at setup time.

### 4.1 Prepare

On the second StartOS device (same version, but updated via OTA):

- Confirm at least one service is installed and running. Note the service name and any user-visible data in it (e.g. a Bitcoin wallet name, a Nextcloud file).
- Create a fresh encrypted backup to the external USB drive.
- Note the second device's admin password and hostname — you will use them to log in after restore.
- Unplug the USB backup drive from the source device.

### 4.2 Re-install the target device from USB

Repeat §1.2 and §1.3 on the target device (USB → installer → internal disk). You do **not** need to repeat §1.1.

- [ ] Re-install completes. Reboot into the setup wizard

### 4.3 Restore flow

Plug the external USB backup drive into the target device. In the setup wizard on the laptop browser:

1. Language: English → Drive: internal disk → Mode: **Restore From Backup** → **Physical drive** → pick the USB backup drive.

- [ ] The USB backup drive appears in the drive list with the correct source-device server name and a timestamp within the last hour

2. Enter a **wrong** password (`wrong`) and submit.

- [ ] UI shows an "incorrect password" error inline, wizard does not crash or hang

3. Enter the second server's admin password and submit.

- [ ] A progress indicator advances past 0%, no errors in the UI
- [ ] On completion the device shows "Continue to Login"

4. From the laptop, browse to `http://<source-hostname>.local` and log in with the source-device admin password.

- [ ] Login succeeds
- [ ] Services page lists the previously installed service from the source device in its previous state (running/stopped)
- [ ] Clicking the service and opening its interface loads its UI, and the user-visible data noted in §4.1 is present
- [ ] System → General Settings → Server Name shows the hostname of the source device

---

## 5. Transfer from old device

This is a **third full install cycle**. It validates the data-drive transfer path, which is distinct from a backup restore: it moves the original drive's data directly, and marks the old drive as "do not boot".

### 5.1 Prepare

1. From the admin UI of the second (known-good) StartOS device, shut it down cleanly (System → Shutdown). Wait until it is fully powered off.
2. Physically remove the second device's **data drive** and attach it to the target device (internal SATA/NVMe slot if available, otherwise a USB-to-SATA adapter is acceptable for the test).
3. Note the second device's admin password and hostname.

### 5.2 Re-install the target device from USB

Repeat §1.2 and §1.3 on the target device, installing to the target's **internal** disk, not the transferred drive.

- [ ] The installer lists both disks and correctly identifies the internal disk as the install target; the transferred drive is not offered as an install target or is clearly labeled as containing existing StartOS data
- [ ] Install completes. Reboot into the setup wizard

### 5.3 Transfer flow

In the setup wizard on the laptop browser:

1. Language: English → Drive: internal disk → Mode: **Transfer Data**.

- [ ] The transferred drive appears in the drive list with the source device's vendor/model
- [ ] A drive with no prior StartOS data (the fresh internal disk) does **not** appear in this list

2. Select the transferred drive.

- [ ] A warning dialog appears containing the text "do not attempt to boot into it again as a Start9 Server"

3. Click **Continue**. Enter `QaTestPass!1` as the new admin password. Submit. (A transfer offers no server-name field — the transferred server keeps the name it had.)

- [ ] Transfer progresses, no errors
- [ ] Device reboots into the admin UI on completion

4. Browse to `http://<source-hostname>.local` — the transferred server keeps the source device's name — and log in with `QaTestPass!1`.

- [ ] Login succeeds with the new password (not the old source password)
- [ ] System → General Settings → Server Name shows the hostname of the source device
- [ ] Services page lists the same service that was on the source device
- [ ] The service's interface loads and the user-visible data noted in §4.1 is present

### 5.4 Post-transfer verification

SSH into the target and run:

```bash
lsblk -o NAME,SIZE,MOUNTPOINTS
sudo btrfs filesystem show
```

- [ ] The transferred drive appears in `lsblk` but is **not** mounted as the active data drive — the internal disk is
- [ ] `btrfs filesystem show` lists the active filesystem on the internal disk
