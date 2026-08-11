# Bitcoin Guides

Guides for running Bitcoin and Lightning on your own hardware with StartOS — from choosing a wallet to managing channels to migrating from another node platform.

## Why Run Your Own Node?

When you use a Bitcoin wallet without your own node, you are trusting someone else's server to tell you your balance, broadcast your transactions, and report on the state of the network. That server operator can see which addresses belong to you, track your transaction history, and potentially censor or delay your transactions.

Running your own Bitcoin node means your wallet talks directly to the Bitcoin network through your own verified copy of the blockchain. No one can see your addresses, censor your transactions, or lie to you about your balance. It is the difference between trusting and verifying.

StartOS makes this practical. Install a Bitcoin node from the StartOS Marketplace and it handles the configuration, networking, and storage — no command line required. Add an Electrum server (such as Fulcrum) and most wallets can connect to your node with a single address. Add LND or Core Lightning and you have a full Lightning node for instant, low-fee payments.

## What's in This Guide

- **[Archival vs Pruned Nodes](archival-vs-pruned.md)** — The trade-offs between archival and pruned nodes, and how StartOS makes pruned nodes work seamlessly with multiple downstream services.

- **[Electrum Servers](electrum-servers.md)** — What an Electrum server is, why you need one, and which implementations are available on StartOS.

- **[Bitcoin Wallets](bitcoin-wallets.md)** — An index of on-chain wallets that connect to your own Bitcoin node or Electrum server, with platforms, connection methods, and links to upstream docs.

- **[Lightning Wallets](lightning-wallets.md)** — Native apps and self-hosted web tools for managing your LND or Core Lightning node, including RTL, ThunderHub, Zeus, Alby Hub, and more.

- **[Migrating LND to StartOS](lnd-migration.md)** — How to transfer your LND node from Umbrel, myNode, or another StartOS server without closing channels.

## Supported Implementations

StartOS supports multiple Bitcoin and Lightning node implementations. You are not locked into a single stack — choose the implementation that fits your needs.

### Bitcoin Nodes

The Bitcoin service on StartOS is available in multiple flavors (implementations). The service is called **Bitcoin** regardless of which flavor you install — the flavor determines the underlying software.

| Flavor            | Description                                                                                      |
| ----------------- | ------------------------------------------------------------------------------------------------ |
| **Bitcoin Core**  | The reference implementation — validates blocks, relays transactions, serves wallet data via RPC |
| **Bitcoin Knots** | A Bitcoin Core derivative with additional configuration options and policy controls              |

Both flavors provide the same RPC interface used by wallets and Electrum servers.

### Lightning Nodes

| Service            | Description                                                                                |
| ------------------ | ------------------------------------------------------------------------------------------ |
| **LND**            | Lightning Labs' implementation — the most widely supported by wallets and management tools |
| **Core Lightning** | Blockstream's implementation — includes the CLN Application web dashboard                  |

## Learn More

- [StartOS Documentation](/start-os/) — Installing StartOS, user manual, networking, backups
- [Contact Support](https://start9.com/contact) — Get help from the Start9 team
