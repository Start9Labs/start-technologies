# Bitcoin Wallets

An index of Bitcoin wallets that support connecting to your own Bitcoin node (via RPC) or your own Electrum server (such as Fulcrum). Each entry lists supported platforms, connection method, Tor support, and links to upstream documentation.

For Lightning wallets that connect to your own LND or Core Lightning node, see [Lightning Wallets](lightning-wallets.md).

## Connection Methods

There are two ways a wallet can connect to your self-hosted Bitcoin infrastructure:

- **Bitcoin RPC** — The wallet connects directly to your Bitcoin node using its JSON-RPC interface. This gives the wallet full access to your node's blockchain data without any additional indexing service.
- **Electrum server** — The wallet connects to an Electrum-protocol server (such as Fulcrum) that sits in front of your Bitcoin node. The Electrum server indexes the blockchain and provides fast address lookups. Most wallets use this method.

## Summary

| Wallet                              | Platforms                           | Connects to     | Tor |
| ----------------------------------- | ----------------------------------- | --------------- | --- |
| [BitBoxApp](#bitboxapp)             | Android, iOS, Linux, macOS, Windows | Electrum server | Yes |
| [Bitcoin Keeper](#bitcoin-keeper)   | Android, iOS                        | Electrum server | Yes |
| [Blockstream App](#blockstream-app) | Android, iOS, Linux, macOS, Windows | Electrum server | Yes |
| [BlueWallet](#bluewallet)           | Android, iOS                        | Electrum server | Yes |
| [Bull Wallet](#bull-wallet)         | Android, iOS                        | Electrum server | Yes |
| [BTCPay Server](#btcpay-server)     | Web (self-hosted)                   | Bitcoin RPC     | Yes |
| [Electrum](#electrum)               | Android, Linux, macOS, Windows      | Electrum server | Yes |
| [Envoy](#envoy)                     | Android, iOS                        | Electrum server | Yes |
| [FullyNoded](#fullynoded)           | iOS, macOS                          | Bitcoin RPC     | Yes |
| [Liana](#liana)                     | Linux, macOS, Windows               | Both            | No  |
| [Nunchuk](#nunchuk)                 | Android, iOS, Linux, macOS, Windows | Both            | Yes |
| [Sparrow](#sparrow)                 | Linux, macOS, Windows               | Both            | Yes |
| [Trezor Suite](#trezor-suite)       | Linux, macOS, Windows               | Electrum server | Yes |
| [Wasabi](#wasabi)                   | Linux, macOS, Windows               | Bitcoin RPC     | Yes |

## BitBoxApp

- **Platforms:** Android, iOS, Linux, macOS, Windows
- **Connects to:** Electrum server
- **Tor:** Yes — supports Tor proxy and .onion addresses natively

The BitBoxApp is the companion software for BitBox hardware wallets. It supports connecting to your own Electrum server via the app's advanced settings.

- [BitBoxApp downloads](https://bitbox.swiss/download/)
- [Connecting to your own full node](https://support.bitbox.swiss/connecting-the-bitboxapp-to-your-own-full-node)

## Bitcoin Keeper

- **Platforms:** Android, iOS
- **Connects to:** Electrum server
- **Tor:** Yes

Bitcoin Keeper is an open-source mobile wallet with multisig support. It provides an Electrum Server Management interface for connecting to your own server.

- [Bitcoin Keeper website](https://bitcoinkeeper.app/)
- [Source code](https://github.com/bithyve/bitcoin-keeper)

## Blockstream App

- **Platforms:** Android, iOS, Linux, macOS, Windows
- **Connects to:** Electrum server
- **Tor:** Yes — built-in "Connect with Tor" toggle

Blockstream App supports connecting to your own Electrum server and has a built-in Tor toggle for private connections.

- [Blockstream App downloads](https://blockstream.com/app/)

## BlueWallet

- **Platforms:** Android, iOS
- **Connects to:** Electrum server
- **Tor:** Yes — native Tor on both Android and iOS

BlueWallet is a popular mobile Bitcoin wallet that supports connecting to your own Electrum server. It can scan QR codes for Tor-based node connections.

- [BlueWallet website](https://bluewallet.io/)
- [Electrum server configuration](https://github.com/BlueWallet/BlueWallet/wiki/Electrum-servers-pool)

## Bull Wallet

- **Platforms:** Android, iOS
- **Connects to:** Electrum server
- **Tor:** Yes

Bull Wallet by Bull Bitcoin is a privacy-focused mobile wallet that combines on-chain Bitcoin, Lightning (via Boltz atomic swaps), and Liquid support. It supports connecting to your own Electrum server and works as a companion app for hardware wallets with watch-only imports and air-gapped signing (Coldcard Q).

- [Bull Wallet website](https://www.bullbitcoin.com/blog/bull-by-bull-bitcoin)
- [Source code](https://github.com/SatoshiPortal/bullbitcoin-mobile)

## BTCPay Server

- **Platforms:** Web (self-hosted)
- **Connects to:** Bitcoin RPC
- **Tor:** Yes — built-in Tor support in the Docker deployment

BTCPay Server is a self-hosted payment processor, but it also includes a full on-chain Bitcoin wallet backed by your Bitcoin node. The built-in wallet supports hot wallets, watch-only wallets (xpub import with external signing via PSBT), coin selection, and payment batching. BTCPay Server is also available as a service on the StartOS Marketplace.

BTCPay Server also supports Lightning — see [Lightning Wallets](lightning-wallets.md#btcpay-server).

- [BTCPay Server website](https://btcpayserver.org/)
- [Wallet documentation](https://docs.btcpayserver.org/Wallet/)

## Electrum

- **Platforms:** Android, Linux, macOS, Windows
- **Connects to:** Electrum server
- **Tor:** Yes — supports SOCKS5 proxy for Tor connections

Electrum is the original Electrum-protocol wallet. Connecting to your own Electrum server is a core feature — simply specify your server address in the network settings.

- [Electrum downloads](https://electrum.org/#download)
- [Using Electrum through Tor](https://electrum.readthedocs.io/en/latest/tor.html)

## Envoy

- **Platforms:** Android, iOS
- **Connects to:** Electrum server
- **Tor:** Yes — built-in Tor toggle ("Improved Privacy" mode)

Envoy is the companion app for the Passport hardware wallet by Foundation Devices, but also works as a standalone wallet. It supports connecting to your own Electrum server with native Tor support.

- [Envoy downloads](https://foundation.xyz/envoy/)
- [Envoy setup documentation](https://docs.foundation.xyz/envoy/setup/)

## FullyNoded

- **Platforms:** iOS, macOS
- **Connects to:** Bitcoin RPC
- **Tor:** Yes — integrated Tor support

FullyNoded connects directly to your Bitcoin node over Tor using the RPC interface. No Electrum server required. Connection can be configured via QR code or manual entry.

- [FullyNoded website](https://fullynoded.app/)
- [Connect your node](https://fonta1n3.github.io/FullyNoded/Docs/Bitcoin-Core/Connect.html)

## Liana

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Both — Electrum server and Bitcoin RPC
- **Tor:** No — external Tor proxy required for remote connections

Liana is focused on timelocked recovery and inheritance use cases. It supports both Electrum server and Bitcoin RPC connections, and can install a pruned Bitcoin node automatically for local use.

- [Liana website](https://wizardsardine.com/liana/)
- [Source code](https://github.com/wizardsardine/liana)

## Nunchuk

- **Platforms:** Android, iOS, Linux, macOS, Windows
- **Connects to:** Both — Electrum server and Bitcoin RPC
- **Tor:** Yes

Nunchuk is a multisig-focused wallet available on all major platforms. It supports connecting to your own Electrum server or Bitcoin node.

- [Nunchuk website](https://nunchuk.io/)
- [Custom node connection (v0.9.7)](https://medium.com/nunchuk/nunchuk-0-9-7-6b74b3848c3d)

## Sparrow

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Both — Electrum server and Bitcoin RPC
- **Tor:** Yes — bundled internal Tor proxy that activates automatically for .onion addresses

Sparrow is a full-featured desktop wallet widely regarded as the power-user choice. It supports both Bitcoin RPC and Electrum server connections. When you enter a .onion address, Sparrow automatically routes through its built-in Tor proxy.

- [Sparrow downloads](https://sparrowwallet.com/download/)
- [Connect to your node](https://sparrowwallet.com/docs/connect-node.html)

## Trezor Suite

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Electrum server
- **Tor:** Yes — desktop app supports .onion addresses for custom backends

Trezor Suite is the companion software for Trezor hardware wallets. The desktop app supports connecting to a custom Electrum server backend. The web app and mobile app (Trezor Suite Lite) do not support custom backends.

- [Trezor Suite downloads](https://trezor.io/trezor-suite)
- [Connect Trezor Suite to your own node](https://trezor.io/learn/security-privacy/how-trezor-keeps-you-safe/connect-trezor-suite-to-your-node)
- [Full node via Electrum server](https://trezor.io/learn/supported-assets/bitcoin/full-node-via-electrum-server)

## Wasabi

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Bitcoin RPC
- **Tor:** Yes — Tor is built-in and enabled by default for all network traffic

Wasabi Wallet is a privacy-focused desktop wallet. It routes all traffic through Tor by default and supports connecting to your own Bitcoin node via RPC.

- [Wasabi downloads](https://wasabiwallet.io/)
- [Bitcoin full node integration](https://docs.wasabiwallet.io/using-wasabi/BitcoinFullNode.html)
