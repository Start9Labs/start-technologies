# Migrating LND to StartOS

How to transfer your LND node — including on-chain funds and open Lightning channels — from another platform to StartOS without closing channels.

> [!WARNING]
>
> After migrating your LND wallet to StartOS, **never restart your old node**. Turning on your old node can broadcast old channel states and result in loss of funds.

## Supported Source Platforms

StartOS's LND service can pull wallet and channel data directly from the following platforms over your local network:

- **Umbrel** 1.x
- **myNode**
- **another StartOS server** running StartOS 0.4.0 or later. A server still on 0.3.5.x must [update to 0.4.0](/start-os/update-040.html) first.

If your source platform is not listed, see [Other Platforms](#other-platforms) below.

## Prerequisites

- Both devices (source node and StartOS server) must be on the **same local network**.
- Your source node must be **running and reachable** when you schedule the migration and when you start LND — the copy happens at startup.
- You need your source node's **local IP address or `.local` hostname** (check your router's admin page if unsure).
- You need the password the migration signs in with:

| Source          | Password to enter                                                               |
| --------------- | ------------------------------------------------------------------------------- |
| Umbrel          | The password for your Umbrel dashboard, which is also its SSH password          |
| myNode          | The password for myNode's `admin` user, used for both SSH and the web interface |
| Another StartOS | That server's master password                                                   |

You do not need your source node's LND wallet password. The migration reads it from the origin and carries it across, so StartOS can unlock the wallet you already have.

## Migrating

Install LND from the StartOS Marketplace and **do not start it**. Its **Initialize Wallet** task offers **Migrate from Umbrel**, **Migrate from myNode** and **Migrate from StartOS**; LND's own instructions in StartOS walk through the task, the checks it makes before anything is copied, and what to watch while it runs. The task refuses to migrate onto an existing wallet, so if you already chose **Start Fresh**, uninstall LND and install it again first.

Nothing is copied when you run the task. The migration runs when you **start LND**: it stops the services on your source node, copies LND's wallet and channel database across your local network, converts the database, and then brings LND online. That can take anywhere from a few minutes to several hours, depending on the size of the channel database and the speed of your network and the source node's disk. Stopping LND part-way is safe; it picks up where it left off on the next start.

### Disconnect the Old Node

Once LND is online with your migrated wallet, **shut down and disconnect your old node**. Running two nodes with the same channel state results in force-closures and potential loss of funds.

The migration stops the source node's services before copying — and on a StartOS source that stop persists across reboots — but only powering the device down guarantees it stays off. In particular, rebooting a migrated Umbrel or myNode brings its LND back.

> [!WARNING]
>
> Never restart your old node after the migration has completed. If you need to go back to your old node for any reason, do **not** start LND on StartOS first.

### Back Up

Take a [StartOS backup](/start-os/backups.html), and set up [continuous backups](continuous-backups.md). Your channel funds are recoverable only from LND's static channel backup, and a StartOS backup holds it only as of the moment it was taken — a continuous backup keeps a copy current on storage you choose.

## Other Platforms

There is no built-in migration for platforms outside the list above — including RaspiBlitz, which earlier StartOS releases supported and current ones do not.

The safe route from an unsupported platform is to **close your channels on the old node first**, letting the balances settle on-chain, and then recover the on-chain funds on StartOS. Run **Initialize Wallet → Start Fresh** on StartOS and send the funds over from your old wallet, or restore your old node's seed into an on-chain wallet of your choice. This costs you your channels and the fees to re-open them, but it carries none of the force-close risk of moving channel state by hand.

Copying an LND data directory across by hand is possible — it is what the built-in migrations do — but there is no supported path for it, and a partial or inconsistent copy of a channel database force-closes channels rather than failing safely. If you intend to try it anyway, the source and destination paths each migration uses are documented in the [LND package README](https://github.com/Start9Labs/lnd-startos#initialize-wallet).

## If Channels Force-Close

Force-closes after a migration usually mean the old node was restarted afterwards, or the channel database was damaged in transfer. Force-closed channels cannot be recovered; the funds return to your on-chain wallet once the timelock expires.
