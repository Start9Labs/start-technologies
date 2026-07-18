# Update to StartOS 0.4.0

StartOS 0.4.0 is a completely new operating system. It will eventually be available as a normal over-the-air update, but for early access it requires a USB flash install. This guide walks you through the process step by step.

> [!WARNING]
> This is early-access software. Bugs are still possible. Follow every step carefully — skipping the service update or backup steps can result in **permanent data loss**.
>
> Backups from StartOS 0.3.5.1 **cannot** be restored onto 0.4.0, and vice versa. The 0.3.5.1 backup you create before migrating can only be used to roll back to 0.3.5.1.

## Before You Begin

StartOS 0.4.0 is currently in beta. The latest beta release is available on the [GitHub releases page](https://github.com/Start9Labs/start-technologies/releases/tag/v0.4.0-beta.9).

### Services with special handling

The following services cannot be migrated automatically. Review these before starting the update:

- **Embassy Pages** — Retired and replaced by **Start9 Pages**. Embassy Pages will survive the update but will no longer receive updates. Uninstall it, install Start9 Pages from the marketplace, and re-add your content.

- **Ghost** — Completely redesigned for StartOS 0.4.0 and incompatible with the old version. Before updating, open your old Ghost admin UI and use Ghost's built-in **Export** tool to download your content. After updating, install the new Ghost from the marketplace and use Ghost's built-in **Import** tool to restore your content.

- **Synapse** — The old Synapse was Tor-only. The new Synapse is clearnet-only. These are different services now with no migration path.

- **Jam** — Jam's backend, JoinMarket, is being replaced by a separate reimplementation (JoinMarket NG) for technical and security reasons, making Jam defunct on StartOS v0.3.5.1 and unavailable on v0.4.0 until that backend matures and a new version of Jam is built for it. You should back up your seed, move out any spendable funds (fidelity-bond funds stay locked until expiry), and uninstall Jam prior to updating to v0.4.0.

### LAN addresses are changing

In StartOS 0.3.x, each service had its own `.local` address (e.g. `longexamplepublickey.local`). In 0.4.0, services are accessed on unique ports of your server's main `.local` address (e.g. `adjective-noun.local:4545`). Your old per-service `.local` addresses will no longer exist after the update.

If you use a password manager, before updating, make sure your saved passwords have clear names/labels (not just the current `.local` URLs) so that you can identify them later and save the new URLs.

> [!TIP]
> This change is a big improvement for Windows users — per-service `.local` addresses required Bonjour and other workarounds that are no longer needed.

## Step 1: Flash the USB Drive

Download the 0.4.0-beta ISO for your platform from the [GitHub releases page](https://github.com/Start9Labs/start-technologies/releases/tag/v0.4.0-beta.9). Under "ISO Downloads" at the top of the release notes, select the ISO for your hardware:

- **Server One (2023)** or other x86_64 hardware — download the **x86_64 (AMD64)** ISO
- **Server Pure** — download the **x86_64 (AMD64) Slim (FOSS-only)** ISO
- **Raspberry Pi** — not yet available for 0.4.0, but support is coming soon

Flash the ISO to a USB drive following the [Download](installing-startos.md#download) and [Flash](installing-startos.md#flash) sections of the install guide.

## Step 2: Update to StartOS 0.3.5.1

You must be running **StartOS 0.3.5.1** before updating to 0.4.0. If you are on an older version, update to 0.3.5.1 first using the normal [update mechanism](updating-startos.md).

## Step 3: Update All Services

On StartOS 0.3.5.1, update **all installed services** to their latest available versions. Start with services at the base of the dependency tree and work upward — for example, update Bitcoin before LND, and LND before RTL.

> [!WARNING]
> This step is **required**. If you do not update services before migrating, they may fail to migrate on 0.4.0, potentially requiring you to roll back to 0.3.5.1 or lose data entirely.

The one exception is **Bitcoin**, which can safely remain at version 28.x or 29.x. All other services must be on their latest version.

## Step 4 (Optional): Add an SSH Key

If you haven't already, [add an SSH key](ssh.md) to your server. If something goes wrong during the migration, SSH access makes it much easier to debug.

## Step 5: Uninstall Unneeded Services

Every installed service must be migrated, and each one adds to the total migration time. If there are services you don't actually use, it is much faster to uninstall them now and install fresh on 0.4.0 afterward.

## Step 6: Stop All Services

Stop all remaining services and wait for each one to fully stop before proceeding. This ensures no new data is written before the backup.

## Step 7: Create a Full System Backup

With all services stopped, create a [full system backup](/0.3.5.x/user-manual/backups/backup-create.html). Back up every service.

> [!WARNING]
> Do **not** skip this step. Migration failures are a real possibility during beta, and without a backup your data could be lost permanently.

## Step 8: Shut Down the Server

Shut down the server through the StartOS UI.

## Step 9: Boot from USB

1. Insert the flashed USB drive into your server.

1. Power on the server.

1. The installer should boot from the USB drive and become available at `http://start.local`.

> [!TIP]
> If the installer fails to boot and instead your normal StartOS boots, it means you will need to attach a monitor and keyboard (Kiosk mode) in order to enter the BIOS settings to change the boot priorities. The Server Pure should always boot from USB if present. For the Server One, this is done by hitting the ESC key repeatedly at boot time until the BIOS appears. Arrow over to the boot tab, and change Boot Option #1 to your inserted USB thumb drive, then restart.

## Step 10: Run the Installer

1. Select your language.

1. Select the **OS drive** and the **data drive**. These can be the same drive if your server only has one. Double-check that you have selected the correct drive for each.

   > [!WARNING]
   > You must select the **same drive layout** you had on 0.3.5.1. If 0.3.5.1 (OS and data) lived on a single drive, select **that same drive for both** the OS drive and the data drive. If your 0.3.5.1 data was on a separate drive, select a different drive for the OS. Choosing a different layout than your existing install cannot preserve your data, and the installer will refuse rather than erase the drive.

1. When prompted, select **Preserve** to keep your existing data.

   > [!WARNING]
   > If you do not select "Preserve", all data on the drive will be erased.

1. Optionally set a new password, or skip to keep your current password.

## Step 11: Wait

The migration process can take **hours**, depending on how much data you have. Be patient and do not power off or unplug your server.

> [!TIP]
> Expect progress to sit at **85%** for a long time — potentially hours. This is when your installed packages are being migrated to the 0.4.0 format, and the time scales with how many packages you have and how much data each one contains. The installer is not stuck.

## Step 12: Reboot

When the update is complete, follow the on-screen instructions to remove the USB drive and reboot.

## Step 13: Update All Services

Every installed service will have an update available for the 0.4.0 marketplace. Update **all** of them — including Bitcoin — before doing anything else. The 0.4.0 versions are repackaged for the new system, even if the underlying software version is the same.

## Step 14: Start All Services

Once all services are updated, you can start them. Wait for all services to fully start and confirm they are running correctly.

## Step 15: Create a Backup!

Create a [full system backup](backup-create.md). Ideally this is to a separate drive (or network folder) than 0.3.5.

> [!WARNING]
> 0.3.5 backups and 0.4.0 backups are **ENTIRELY INCOMPATIBLE**. 0.3.5 backups **cannot** be restored onto 0.4.0. and 0.4.0 backups **cannot** be restored on 0.3.5.1.

If backing up to the same drive as 0.3.5, a new subfolder will be created automatically. Just be sure the drive has enough space to hold both complete backups.

Remember, regenerable indexes, such as the Bitcoin block chain and Electrs/Fulcrum indexes, are _not_ backed up. This is a good thing.

Depending on the speed of your drive, plan on 3-5 minutes per GB of backup data. So 100 GB of data could take over 8 hours. 0.4.0 backups are _differential_ in nature, so future backups will only include new or deleted files and therefore should be much faster.

## Post-Migration Notes

### Tor Cleanup

During migration, the **Tor** service is automatically installed with all your existing onion addresses intact. However, Tor is rarely needed in StartOS 0.4.0 — most users will be better served by other networking options.

You are encouraged to review your service interfaces and delete any Tor addresses you do not intend to use.

### Explore the New System

Take time to explore the new UI and read the documentation. StartOS 0.4.0 is a fundamentally different system from 0.3.x.
