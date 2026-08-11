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
- Your source node must be **running and reachable** at the time of migration.
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

### 2. Run the Migration

Open LND on your StartOS server, go to **Actions**, and run **Initialize Wallet**. Under **Initialization Method**, choose the option matching your source platform:

- **Migrate from Umbrel**
- **Migrate from myNode**
- **Migrate from StartOS**

Enter your source node's address and password, then submit.

### 3. Wait for the Migration to Complete

The migration shuts down the services on your source node, copies LND's wallet, channel database and configuration across, and adopts its wallet password. This can take anywhere from a few minutes to a few hours, depending on the size of your channel database and the speed of your network and the source node's disk. Leave the action running until it reports success.

### 4. Disconnect the Old Node

Once the migration reports success, **shut down and disconnect your old node** before proceeding. This is critical — running two nodes with the same channel state will result in force-closures and potential loss of funds.

The migration stops the source node's services as its first step, and the StartOS source is left with LND uninstalled, but only powering the device down guarantees it stays off.

### 5. Choose a Bitcoin Backend and Start LND

With the old node safely shut down, complete LND's second critical task by running **Bitcoin Backend**: pick **Bitcoin** if you run a Bitcoin node on this server (recommended), or **Neutrino** to use the built-in light client.

Then start LND. Umbrel, myNode and pre-0.21 StartOS nodes all run LND's older `bolt` database, which StartOS converts to SQLite before the service comes up. On a large channel database this conversion can itself take hours; LND reports which stage it is on while it runs, and the service will not finish starting until it is done. Leave it alone until it does.

LND will then begin syncing and reconnecting to your peers with the migrated channel state.

> [!WARNING]
>
> Never restart your old node after the migration has completed. If you need to go back to your old node for any reason, do **not** start LND on StartOS first.

## Other Platforms

There is no built-in migration for platforms outside the list above — including RaspiBlitz, which earlier StartOS releases supported and current ones do not.

The safe route from an unsupported platform is to **close your channels on the old node first**, letting the balances settle on-chain, and then recover the on-chain funds on StartOS. Run **Initialize Wallet → Start Fresh** on StartOS and send the funds over from your old wallet, or restore your old node's seed into an on-chain wallet of your choice. This costs you your channels and the fees to re-open them, but it carries none of the force-close risk of moving channel state by hand.

Copying an LND data directory across by hand is possible — it is what the built-in migrations do — but there is no supported path for it, and a partial or inconsistent copy of a channel database force-closes channels rather than failing safely. If you intend to try it anyway, the source and destination paths each migration uses are documented in the [LND package README](https://github.com/Start9Labs/lnd-startos#initialize-wallet).

## Troubleshooting

**The migration option is missing, or the action refuses to run** — LND already has a wallet, or the service has been started. Uninstall LND and install a fresh copy from the StartOS Marketplace.

**Migration times out or fails** — Ensure both devices are on the same local network and that the source node is running. Double-check the address and password. A failed migration leaves no wallet behind, so you can correct the details and run the action again.

**Channels force-close after migration** — This usually means the old node was restarted after migration, or the channel database was corrupted during transfer. Unfortunately, force-closed channels cannot be recovered — the funds will be returned to your on-chain wallet after the timelock expires.
