# Update to StartOS 0.4.0

StartOS 0.4.0 is a completely new operating system. There are two ways to update to it, both of which preserve your services and data and migrate them to the new format — but they are not equally reliable:

- A **flash update** — you boot the 0.4.0 installer from a USB drive and install it over your existing server. **This is the recommended method.**
- An **over-the-air update** — your server downloads 0.4.0 itself and applies it on the next restart.

Both are covered in [Install StartOS 0.4.0](#install-startos-040) below.

> [!NOTE]
> **Raspberry Pi cannot update over the air.** A Pi flash update uses the 0.4.0 Raspberry Pi microSD image rather than a USB installer. Complete [Prepare Your Server](#prepare-your-server) below, then follow the [Raspberry Pi flashing instructions](installing-startos.md#raspberry-pi).

The preparation steps are **not optional**: complete [Prepare Your Server](#prepare-your-server) before updating.

> [!WARNING]
> Follow every step carefully — skipping the service update or backup steps can result in **permanent data loss**.
>
> Backups from StartOS 0.3.5.1 **cannot** be restored onto 0.4.0, and vice versa. The 0.3.5.1 backup you create before migrating can only be used to roll back to 0.3.5.1.

## Before You Begin

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

## Prepare Your Server

Complete all of these steps before updating. They apply to every platform, Raspberry Pi included.

### Step 1: Update to StartOS 0.3.5.1

You must be running **StartOS 0.3.5.1** before updating to 0.4.0. If you are on an older version, update to 0.3.5.1 first using the normal [0.3.x update mechanism](/0.3.5.x/user-manual/updating.html).

### Step 2: Update All Services

On StartOS 0.3.5.1, update **all installed services** to their latest available versions. Start with services at the base of the dependency tree and work upward — for example, update Bitcoin before LND, and LND before RTL.

> [!WARNING]
> This step is **required**. If you do not update services before migrating, they may fail to migrate on 0.4.0, potentially requiring you to roll back to 0.3.5.1 or lose data entirely.

Bitcoin may safely remain at 28.x or 29.x, but you MUST update to the latest **minor** version of your selected major version. All other services must be on their latest version.

### Step 3 (Optional): Add an SSH Key

If you haven't already, [add an SSH key](ssh.md) to your server. If something goes wrong during the migration, SSH access makes it much easier to debug.

### Step 4: Uninstall Unneeded Services

Every installed service must be migrated, and each one adds to the total migration time. If there are services you don't actually use, it is much faster to uninstall them now and install fresh on 0.4.0 afterward.

### Step 5: Stop All Services

Stop all remaining services and wait for each one to fully stop before proceeding. This ensures no new data is written before the backup.

### Step 6: Create a Full System Backup

With all services stopped, create a [full system backup](/0.3.5.x/user-manual/backups/backup-create.html). Back up every service.

> [!WARNING]
> Do **not** skip this step. Migration failures are possible, and without a backup your data could be lost permanently.

## Install StartOS 0.4.0

> [!WARNING]
> **Flash updating is the more reliable method, and the one we recommend.** If you can physically reach your server and boot it from a USB drive, update that way.
>
> The over-the-air update needs neither a USB drive nor physical access, but it is the more failure-prone of the two. If you take it, the backup from Step 6 is your fallback.

### Step 7: Install 0.4.0

Pick your method below. Everything above applies to both, and the two paths rejoin at [Step 8](#step-8-wait-for-the-migration).

{{#tabs}}
{{#tab name="Flash Update (Recommended)"}}

1. Flash the 0.4.0 installer to a USB drive from any computer, following the [Download](installing-startos.md#download) and [Flash](installing-startos.md#flash) sections of the install guide. Your server can keep running while you do this.

   > [!NOTE]
   > On a Raspberry Pi, there is no USB installer — flash the 0.4.0 Raspberry Pi image to the Pi's microSD card instead. Follow the [Raspberry Pi flashing instructions](installing-startos.md#raspberry-pi) in place of the steps below, then continue with [Step 8](#step-8-wait-for-the-migration) — a Pi reaches the same migration progress screen.

1. Shut down your server through the StartOS UI.

1. Insert the flashed USB drive into your server and power it on. The installer should boot from the USB drive and become available at `http://start.local`.

   > [!TIP]
   > If the installer fails to boot and instead your normal StartOS boots, it means you will need to attach a monitor and keyboard (Kiosk mode) in order to enter the BIOS settings to change the boot priorities. The Server Pure should always boot from USB if present. For the Server One, this is done by hitting the ESC key repeatedly at boot time until the BIOS appears. Arrow over to the boot tab, and change Boot Option #1 to your inserted USB thumb drive, then restart.

1. Select your language.

1. Select the **OS drive** and the **data drive**. These can be the same drive if your server only has one. Double-check that you have selected the correct drive for each.

   > [!WARNING]
   > You must select the **same drive layout** you had on 0.3.5.1. If 0.3.5.1 (OS and data) lived on a single drive, select **that same drive for both** the OS drive and the data drive. If your 0.3.5.1 data was on a separate drive, select a different drive for the OS. Choosing a different layout than your existing install cannot preserve your data, and the installer will refuse rather than erase the drive.

1. When prompted, select **Preserve** to keep your existing data.

   > [!WARNING]
   > If you do not select "Preserve", all data on the drive will be erased.

1. Optionally set a new password, or skip to keep your current password. The migration begins — continue with [Step 8](#step-8-wait-for-the-migration).

{{#endtab}}
{{#tab name="Over the Air"}}

Once StartOS 0.4.0 is available for your server, it is offered under **System → Software Update**. If it is not offered yet, check again later. (Raspberry Pi is never offered the update — use the flash update.)

1. Go to **System → Software Update**, review the release notes, and click **Begin Update**. The download (~3 GB) runs in the background while your server continues running.

1. When the download completes, the System page shows **Update Complete. Restart to apply changes**. Restart your server through the StartOS UI.

1. **Your server does not immediately come back at its old address on this restart.** Wait a few minutes for it to reboot, then go to `http://start.local` — the same address the USB installer uses. The migration begins — continue with [Step 8](#step-8-wait-for-the-migration).

{{#endtab}}
{{#endtabs}}

### Step 8: Wait for the Migration

Both methods converge here: your server is migrating, and shows its progress at `http://start.local`.

1. StartOS converts your system to the 0.4.0 format and then migrates every installed service. This can take **hours**, depending on how much data you have. Be patient and do not power off or unplug your server.

   > [!TIP]
   > Expect progress to sit at **85%** for a long time — potentially hours. This is when your installed packages are being migrated to the 0.4.0 format, and the time scales with how many packages you have and how much data each one contains. It is not stuck.

1. When the migration is complete, follow the on-screen instructions to reboot. If you updated from a USB installer, remove the drive first — a Pi's microSD card stays in.

1. Once your server has rebooted, go to your server's own address (`https://adjective-noun.local`) — the address you used on 0.3.5.1, not `start.local`.

   **If you get the old 0.3.5.1 interface, a blank page, or a "cannot connect" error, your browser is serving you its cached copy of the old UI.** The server is fine; the page is stale. Any of these will get you the 0.4.0 UI:
   - Open the address in a new private/incognito window.
   - Hard refresh the page:
     - Linux/Windows: `ctrl+shift+R`
     - macOS Firefox: `cmd+shift+R`
     - macOS Safari: `cmd+option+E`, then `cmd+R`
   - Clear your browser's cache, then reload.

   If a hard refresh still shows the old UI, fully quit and restart the browser — browsers cache connections more aggressively than page content.

When you can sign in, continue below.

## After the Update

### Step 9: Update All Services

Every installed service will have an update available for the 0.4.0 marketplace. Update **all** of them — including Bitcoin (again, to the latest **minor** of your selected **major** version) — before doing anything else. The 0.4.0 versions are repackaged for the new system, even if the underlying software version is the same.

### Step 10: Start All Services

Once all services are updated, you can start them. Wait for all services to fully start and confirm they are running correctly.

### Step 11: Create a Backup!

Create a [full system backup](backup-create.md). Ideally this is to a separate drive (or network folder) than 0.3.5.

> [!WARNING]
> 0.3.5 backups and 0.4.0 backups are **ENTIRELY INCOMPATIBLE**. 0.3.5 backups **cannot** be restored onto 0.4.0. and 0.4.0 backups **cannot** be restored on 0.3.5.1.

If backing up to the same drive as 0.3.5, a new subfolder will be created automatically. Just be sure the drive has enough space to hold both complete backups.

Remember, regenerable indexes, such as the Bitcoin block chain and Electrs/Fulcrum indexes, are _not_ backed up. This is a good thing.

Depending on the speed of your drive, plan on 3-5 minutes per GB of backup data. So 100 GB of data could take over 8 hours. 0.4.0 backups are _differential_ in nature, so future backups will only include new or deleted files and therefore should be much faster.

## Post-Migration Notes

### If a Service Fails to Migrate

Check your notifications. A service that fails to migrate raises a notification naming the service and the reason it failed, and a summary notification lists everything that needs re-installing.

Your data is safe — it stays on disk where the service left it. Install the service again from the marketplace and it will pick that data back up.

### Tor Cleanup

During migration, the **Tor** service is automatically installed and started, with all your existing onion addresses intact and reachable as soon as the update finishes. However, Tor is rarely needed in StartOS 0.4.0 — most users will be better served by other networking options.

You are encouraged to review your service interfaces and delete any Tor addresses you do not intend to use.

### Explore the New System

Take time to explore the new UI and read the documentation. StartOS 0.4.0 is a fundamentally different system from 0.3.x.
