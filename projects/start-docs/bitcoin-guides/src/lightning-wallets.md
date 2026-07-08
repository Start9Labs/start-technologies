# Lightning Wallets

An index of Lightning wallets and management tools that support connecting to your own LND or Core Lightning node. This includes both native apps (mobile/desktop) and web-based tools that can be self-hosted or installed as services on StartOS.

For on-chain Bitcoin wallets that connect to your own Bitcoin node or Electrum server, see [Bitcoin Wallets](bitcoin-wallets.md).

## Node Implementations

The wallets and tools below connect to one or more of these Lightning node implementations:

- **LND** — The most widely supported implementation. Wallets connect via its REST or gRPC API using a macaroon for authentication. LND also supports [Lightning Node Connect (LNC)](#lightning-node-connect), a secure remote connection method that works without opening ports or using Tor.
- **Core Lightning (CLN)** — Blockstream's implementation. Wallets connect via CLNRest (the built-in REST plugin) or gRPC.
- **Eclair** — ACINQ's implementation. Fewer wallets support it directly.

### Lightning Node Connect

Lightning Node Connect (LNC) is a connection protocol by Lightning Labs that lets you securely reach your LND node from anywhere without opening ports or configuring Tor. It works by establishing an end-to-end encrypted connection through a relay server using a short pairing phrase. LNC requires [Lightning Terminal (litd)](#lightning-terminal) running alongside your LND node.

LNC is supported by [Zeus](#zeus) and [Lightning Terminal's web UI](https://terminal.lightning.engineering/).

- [How LNC works](https://docs.lightning.engineering/lightning-network-tools/lightning-terminal/lightning-node-connect)
- [Source code](https://github.com/lightninglabs/lightning-node-connect)

## Summary

### Native Apps

| Wallet                    | Platforms    | LND        | CLN | Tor |
| ------------------------- | ------------ | ---------- | --- | --- |
| [BitBanana](#bitbanana)   | Android      | Yes        | Yes | Yes |
| [BlueWallet](#bluewallet) | Android, iOS | Via LNDHub | No  | Yes |
| [FullyNoded](#fullynoded) | iOS, macOS   | Yes        | Yes | Yes |
| [Zeus](#zeus)             | Android, iOS | Yes        | Yes | Yes |

### Web-Based (Self-Hosted)

These tools run as web applications on your server. Several are available as services on the StartOS Marketplace.

| Tool                                      | LND | CLN | Eclair | StartOS |
| ----------------------------------------- | --- | --- | ------ | ------- |
| [Alby Hub](#alby-hub)                     | Yes | No  | No     | Yes     |
| [BTCPay Server](#btcpay-server)           | Yes | Yes | Yes    | Yes     |
| [CLN Application](#cln-application)       | No  | Yes | No     | Yes     |
| [Lightning Terminal](#lightning-terminal) | Yes | No  | No     | Yes     |
| [LNbits](#lnbits)                         | Yes | Yes | Yes    | Yes     |
| [RTL](#rtl)                               | Yes | Yes | Yes    | Yes     |
| [ThunderHub](#thunderhub)                 | Yes | No  | No     | Yes     |

## Native Apps

### BitBanana

- **Platforms:** Android
- **Connects to:** LND, Core Lightning
- **Tor:** Yes — native Tor support

BitBanana is the actively maintained successor to the Zap Android wallet. It connects to LND via lndconnect URI (REST) and to Core Lightning via cln-grpc. Supports BOLT 12, coin control, and NFC.

- [BitBanana website](https://bitbanana.app/)
- [Connect a Lightning node](https://docs.bitbanana.app/setup/connect-a-lightning-node)

### BlueWallet

- **Platforms:** Android, iOS
- **Connects to:** LND (via LNDHub)
- **Tor:** Yes — available in network settings

BlueWallet connects to LND through LNDHub, a middleware layer that sits on top of your LND node and provides account-based access. This makes it good for multi-user setups where one node serves several wallets. LNDHub requires LND and Redis as dependencies.

> [!NOTE]
>
> BlueWallet shut down their hosted custodial LNDHub service in 2023, but the self-hosted open-source LNDHub software still works. You can also use the LNDHub extension in LNbits as an alternative backend.

- [BlueWallet website](https://bluewallet.io/)
- [LNDHub documentation](https://bluewallet.io/lndhub/)

### FullyNoded

- **Platforms:** iOS, macOS
- **Connects to:** LND, Core Lightning
- **Tor:** Yes — Tor is a core design principle; all connections route through Tor

FullyNoded is primarily a Bitcoin wallet, but also supports connecting to LND (via REST over Tor) and Core Lightning. Apple ecosystem only.

- [FullyNoded website](https://fullynoded.app/)
- [Lightning documentation](https://fonta1n3.github.io/FullyNoded/Docs/Lightning.html)

### Zeus

- **Platforms:** Android, iOS
- **Connects to:** LND, Core Lightning, Eclair, LNDHub
- **Tor:** Yes — native on Android; experimental on iOS

Zeus is the most versatile mobile Lightning wallet, supporting three node implementations plus LNDHub accounts. It connects to LND via REST or [Lightning Node Connect (LNC)](#lightning-node-connect), Core Lightning via CLNRest, and Eclair via REST. Zeus can also run an embedded LND node on-device without a remote connection.

- [Zeus website](https://zeusln.com/)
- [Remote connection guides](https://docs.zeusln.app/category/remote-connections/)

## Web-Based (Self-Hosted)

### Alby Hub

- **Platforms:** Web (self-hosted), also available as a desktop app on Linux, macOS, Windows
- **Connects to:** LND (external node via REST), or runs an embedded LDK node
- **Tor:** Yes — works with Tor hidden services when self-hosted
- **StartOS:** Available on the StartOS Marketplace

Alby Hub bridges Lightning to the Nostr ecosystem via the Nostr Wallet Connect (NWC) protocol. It can connect to your existing LND node or run its own embedded LDK node. Useful for connecting Lightning payments to Nostr clients, podcasting apps, and web applications.

- [Alby Hub website](https://albyhub.com/)
- [Setup guide](https://guides.getalby.com/user-guide/alby-hub)
- [Source code](https://github.com/getAlby/hub)

### BTCPay Server

- **Platforms:** Web (self-hosted)
- **Connects to:** LND, Core Lightning, Eclair
- **Tor:** Yes — built-in Tor support in the Docker deployment
- **StartOS:** Available on the StartOS Marketplace

BTCPay Server is a self-hosted payment processor for merchants. Its Lightning integration supports all three major node implementations. Use it to accept Lightning payments through invoices, point-of-sale, e-commerce plugins (WooCommerce, Shopify), and more.

BTCPay Server also has a built-in on-chain wallet — see [Bitcoin Wallets](bitcoin-wallets.md#btcpay-server).

- [BTCPay Server website](https://btcpayserver.org/)
- [Lightning documentation](https://docs.btcpayserver.org/LightningNetwork/)

### CLN Application

- **Platforms:** Web (self-hosted)
- **Connects to:** Core Lightning only
- **Tor:** Depends on host platform configuration
- **StartOS:** Included with Core Lightning on the StartOS Marketplace

The CLN Application is the official Blockstream-maintained web dashboard for Core Lightning nodes. It connects via CLNRest and provides a clean interface for channel management, payments, and node monitoring. On StartOS, it is bundled with the Core Lightning service.

- [Source code](https://github.com/ElementsProject/cln-application)

### Lightning Terminal

- **Platforms:** Web (self-hosted and hosted)
- **Connects to:** LND only
- **Tor:** Yes
- **StartOS:** Available on the StartOS Marketplace

Lightning Terminal (LiT) is Lightning Labs' web dashboard for LND nodes. It bundles Loop (Lightning-to-on-chain swaps), Pool (liquidity marketplace), and Taproot Assets into a single interface. The underlying daemon (`litd`) also enables [Lightning Node Connect (LNC)](#lightning-node-connect), allowing secure remote access to your node without opening ports or using Tor.

Lightning Labs also hosts a web UI at [terminal.lightning.engineering](https://terminal.lightning.engineering/) that connects to your node via LNC.

- [Lightning Terminal documentation](https://docs.lightning.engineering/lightning-network-tools/lightning-terminal/introduction)
- [Source code](https://github.com/lightninglabs/lightning-terminal)

### LNbits

- **Platforms:** Web (self-hosted)
- **Connects to:** LND, Core Lightning, Eclair
- **Tor:** Yes — can be exposed as a Tor hidden service
- **StartOS:** Available on the StartOS Marketplace

LNbits is an accounts and extensions platform that sits on top of your Lightning node. It creates isolated wallet accounts (useful for multi-user setups) and has a rich extension system including paywall, point-of-sale, tipping, and an LNDHub extension that lets you serve BlueWallet and Zeus users from your node.

- [LNbits website](https://lnbits.com/)
- [Backend wallet configuration](https://docs.lnbits.org/guide/wallets.html)
- [Source code](https://github.com/lnbits/lnbits)

### RTL

- **Platforms:** Web (self-hosted)
- **Connects to:** LND, Core Lightning, Eclair
- **Tor:** Yes — can be exposed as a Tor hidden service
- **StartOS:** Available on the StartOS Marketplace

RTL (Ride The Lightning) is the most widely supported web UI for Lightning node management. It supports all three major implementations and provides full channel management, payment tracking, routing fee configuration, and Loop/Pool integration.

- [RTL website](https://www.ridethelightning.info/)
- [Source code](https://github.com/Ride-The-Lightning/RTL)

### ThunderHub

- **Platforms:** Web (self-hosted)
- **Connects to:** LND only
- **Tor:** Yes — can be exposed as a Tor hidden service
- **StartOS:** Available on the StartOS Marketplace

ThunderHub is a modern web UI for LND nodes with strong channel balancing tools, multi-account support, and a polished interface. If you run LND, ThunderHub is a strong alternative to RTL.

- [ThunderHub website](https://thunderhub.io/)
- [Documentation](https://docs.thunderhub.io/)
- [Source code](https://github.com/apotdevin/thunderhub)
