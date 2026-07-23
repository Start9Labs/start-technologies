# Update to StartOS 0.4.0

StartOS 0.4.0 is a completely new operating system. The update is delivered **over the air**: your server updates itself in place through **System → Software Update**, preserving your services and data.

> [!NOTE]
> **Raspberry Pi cannot update over the air.** Updating a Raspberry Pi means reflashing its microSD card with the 0.4.0 Raspberry Pi image. Complete [Prepare Your Server](#prepare-your-server) below, then follow the [Raspberry Pi flashing instructions](installing-startos.md#raspberry-pi).

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

## Update Over the Air

Once StartOS 0.4.0 is available for your server, it is offered under **System → Software Update**. If it is not offered yet, check again later. (Raspberry Pi is not offered the update — follow the [Raspberry Pi flashing instructions](installing-startos.md#raspberry-pi) instead.)

### Step 7: Begin the Update

Go to **System → Software Update**, review the release notes, and click **Begin Update**. The download (~3 GB) runs in the background while your server continues running.

### Step 8: Restart to Apply

When the download completes, the System page shows **Update Complete. Restart to apply changes**. Restart your server through the StartOS UI.

### Step 9: Wait

On this restart, StartOS converts your system to the 0.4.0 format and then migrates every installed service. Your server will be unreachable while the disk layout is converted; once StartOS 0.4.0 boots, an initialization screen at your server's address (`https://adjective-noun.local`) shows migration progress.

The migration can take **hours**, depending on how much data you have. Be patient and do not power off or unplug your server.

When the migration completes, the login page becomes available at the same address. Continue with [After the Update](#after-the-update).

## After the Update

### Step 10: Update All Services

Every installed service will have an update available for the 0.4.0 marketplace. Update **all** of them — including Bitcoin (again, to the latest **minor** of your selected **major** version) — before doing anything else. The 0.4.0 versions are repackaged for the new system, even if the underlying software version is the same.

### Step 11: Start All Services

Once all services are updated, you can start them. Wait for all services to fully start and confirm they are running correctly.

### Step 12: Create a Backup!

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
