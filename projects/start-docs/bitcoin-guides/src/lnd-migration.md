# Migrating LND to StartOS

How to transfer your LND node — including on-chain funds and open Lightning channels — from another platform to StartOS without closing channels.

> [!WARNING]
>
> After migrating your LND wallet to StartOS, **never restart your old node**. Turning on your old node can broadcast old channel states and result in loss of funds.

## Supported Source Platforms

StartOS's LND service can pull wallet and channel data directly from the following platforms over your local network:

- **Umbrel** 1.x
- **myNode**
- **another StartOS server**

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

## Migration Steps

### 1. Install LND on StartOS

Install LND from the StartOS Marketplace, but **do not start it**. LND posts two critical tasks on install and cannot be started until both are done — leave them for now.

The migration refuses to run if a wallet already exists on this server, so if you have already created one with **Start Fresh**, uninstall LND and install a fresh copy.

### 2. Schedule the Migration

Open LND on your StartOS server and run its **Initialize Wallet** task. Under **Initialization Method**, choose the option matching your source platform:

- **Migrate from Umbrel**
- **Migrate from myNode**
- **Migrate from StartOS**

Enter your source node's address and password, then submit. The task signs in to your source node to verify the address and password work — a wrong password or unreachable node fails here, within seconds, and you can correct the details and run the task again. On success the migration is scheduled: nothing has been copied yet, and your source node is still running.

### 3. Choose a Bitcoin Backend

Complete LND's second task by running **Bitcoin Backend**: pick **Bitcoin** if you run a Bitcoin node on this server (recommended), or **Neutrino** to use the built-in light client.

### 4. Start LND — This Runs the Migration

Start LND. The migration runs as part of startup: it stops the services on your source node, copies LND's wallet and channel database across your local network, converts the database to LND's current SQLite backend (Umbrel, myNode and pre-0.21 StartOS nodes all run the older `bolt` format), and then brings LND online. Watch its progress under **Health Checks** — first **Wallet Import**, then **Database Conversion**.

The copy and conversion together can take anywhere from a few minutes to several hours, depending on the size of your channel database and the speed of your network and the source node's disk. Leave LND running until it comes online. Stopping LND mid-migration is safe — it picks up where it left off on the next start.

### 5. Disconnect the Old Node

Once LND is online with your migrated wallet, **shut down and disconnect your old node**. This is critical — running two nodes with the same channel state will result in force-closures and potential loss of funds.

The migration stops the source node's services before copying — and on a StartOS source that stop persists across reboots — but only powering the device down guarantees it stays off. In particular, rebooting a migrated Umbrel or myNode brings its LND back.

LND will then sync and reconnect to your peers with the migrated channel state.

> [!WARNING]
>
> Never restart your old node after the migration has completed. If you need to go back to your old node for any reason, do **not** start LND on StartOS first.

## Other Platforms

There is no built-in migration for platforms outside the list above — including RaspiBlitz, which earlier StartOS releases supported and current ones do not.

The safe route from an unsupported platform is to **close your channels on the old node first**, letting the balances settle on-chain, and then recover the on-chain funds on StartOS. Run **Initialize Wallet → Start Fresh** on StartOS and send the funds over from your old wallet, or restore your old node's seed into an on-chain wallet of your choice. This costs you your channels and the fees to re-open them, but it carries none of the force-close risk of moving channel state by hand.

Copying an LND data directory across by hand is possible — it is what the built-in migrations do — but there is no supported path for it, and a partial or inconsistent copy of a channel database force-closes channels rather than failing safely. If you intend to try it anyway, the source and destination paths each migration uses are documented in the [LND package README](https://github.com/Start9Labs/lnd-startos#initialize-wallet).

## Troubleshooting

**The migration option is missing, or the task refuses to run** — LND already has a wallet, or the service has been started. Uninstall LND and install a fresh copy from the StartOS Marketplace.

**The task fails when submitted** — StartOS could not sign in to your source node. Ensure both devices are on the same local network and the source node is running, then double-check the address and password and run the task again. Nothing has been copied at this point.

**Wallet Import shows failure after starting LND** — the source node stopped being reachable between scheduling and starting (powered off, address changed, or its services shut down by hand). The migration retries a few times on its own; if it keeps failing, LND stops itself and re-posts the **Initialize Wallet** task — bring the source node back online, run the task again with the corrected details, and start LND to retry.

**Channels force-close after migration** — This usually means the old node was restarted after migration, or the channel database was corrupted during transfer. Unfortunately, force-closed channels cannot be recovered — the funds will be returned to your on-chain wallet after the timelock expires.
