# Lightning Wallets

Wallets and management tools that drive a Lightning node you run yourself — LND or Core Lightning on StartOS. Some are apps on your phone or laptop; others install onto the server and are reached in a browser.

Connecting to a Lightning node is not like connecting to an Electrum server: instead of a host and a port, your node hands you a **single URI that already contains its address and a credential**. Start with [Connecting to your node](#connecting-to-your-node) below, then find your wallet. For on-chain wallets, see [Bitcoin Wallets](bitcoin-wallets.md).

## Node implementations

- **LND** — the most widely supported. Wallets connect over its REST or gRPC API, authenticating with a macaroon. Also supports [Lightning Node Connect](#lightning-node-connect), which reaches your node without opening ports or using Tor.
- **Core Lightning (CLN)** — Blockstream's implementation. Wallets connect over CLNrest (its built-in REST plugin) or gRPC, authenticating with a rune. Ships with its own web dashboard on StartOS.
- **Eclair** — ACINQ's implementation. Not packaged for StartOS; a couple of the tools below support it if you run one elsewhere.

## Connecting to your node

### LND

LND publishes two connection interfaces, **REST** and **gRPC LND Connect**. Each carries an `lndconnect://` URI that you copy or scan as a QR code, and which already contains the address, the certificate details the client needs, and your credential — so there is nothing else to fill in.

> [!WARNING]
> That URI embeds your **admin macaroon**, which is full control of the node and its funds. Treat it exactly like a password: never paste it into a chat, a screenshot, or a support ticket. LND's own instructions cover what to do if one is exposed.

Which interface to use depends on the wallet:

- **REST** — what most mobile wallets want. StartOS serves it with your server's own certificate, so leave certificate validation **on** in the wallet and install the [StartOS Root CA](/start-os/trust-ca.html) on the device, exactly as you did for the dashboard in your browser. A custom domain with an ACME certificate needs no such step.
- **gRPC LND Connect** — for clients that speak gRPC. LND serves its own certificate here and the URI carries it, so the client can verify with nothing installed. That makes the URI long — copy it rather than scanning the QR, which will be dense and slow to read.

To reach your node from outside your home, add an onion address or a custom domain to the interface first, then take the URI: it is only as reachable as the address inside it, so a LAN address in a wallet you carry around stops working the moment you leave.

### Core Lightning

Core Lightning authenticates with a **rune** rather than a macaroon, and its **CLNrest** interface publishes a URL with one already embedded — that single URL is usually all a wallet needs. If an app wants a rune supplied separately instead, the **Create Rune** action mints one.

The URL's scheme tells the wallet which protocol to use:

| Scheme             | Used for                                                              |
| ------------------ | --------------------------------------------------------------------- |
| `clnrest+https://` | LAN and clearnet addresses, where StartOS terminates TLS              |
| `clnrest+http://`  | `.onion` addresses, where Tor already encrypts and no TLS is involved |

As with LND's URI, the embedded rune is a credential — anything holding it can spend.

### Lightning Node Connect

Lightning Node Connect (LNC) is a Lightning Labs protocol that reaches your LND node through a relay using a short pairing phrase, end-to-end encrypted so the relay sees nothing. No port forwarding, no domain, no Tor.

It requires [Lightning Terminal](#lightning-terminal) running alongside LND: generate a session with Admin permissions on Lightning Terminal's Connect page, then pair the wallet with the phrase or QR it shows. [Zeus](#zeus) and Lightning Labs' own [web terminal](https://terminal.lightning.engineering/) both support it.

- [How LNC works](https://docs.lightning.engineering/lightning-network-tools/lightning-terminal/lightning-node-connect)

## Summary

### Apps you run on your own devices

| Wallet                    | Platforms    | LND        | CLN | Tor |
| ------------------------- | ------------ | ---------- | --- | --- |
| [BitBanana](#bitbanana)   | Android      | Yes        | Yes | Yes |
| [BlueWallet](#bluewallet) | Android, iOS | Via LNDHub | No  | Yes |
| [FullyNoded](#fullynoded) | iOS, macOS   | Yes        | Yes | Yes |
| [Zeus](#zeus)             | Android, iOS | Yes        | Yes | Yes |

### Tools that run on your server

| Tool                                      | LND | CLN | Eclair |
| ----------------------------------------- | --- | --- | ------ |
| [Alby Hub](#alby-hub)                     | Yes | No  | No     |
| [BTCPay Server](#btcpay-server)           | Yes | Yes | Yes    |
| [CLN Application](#cln-application)       | No  | Yes | No     |
| [Lightning Terminal](#lightning-terminal) | Yes | No  | No     |
| [LNbits](#lnbits)                         | Yes | Yes | Yes    |
| [RTL](#rtl)                               | Yes | Yes | Yes    |

These install from the StartOS Marketplace and wire themselves to your node over the server's internal network — no URI to paste, no certificate to trust, and no credential leaving the machine. Each one's own instructions in StartOS cover its setup.

## Apps you run on your own devices

### BitBanana

- **Platforms:** Android
- **Connects to:** LND, Core Lightning
- **Tor:** Native

The actively maintained successor to the Zap Android wallet, connecting to LND by `lndconnect://` URI over REST, and to Core Lightning over gRPC. Supports BOLT 12, coin control, and NFC.

**To connect:** open LND's **REST** interface on your server and scan the QR into BitBanana.

- [BitBanana website](https://bitbanana.app/)
- [Connect a Lightning node](https://docs.bitbanana.app/setup/connect-a-lightning-node)

### BlueWallet

- **Platforms:** Android, iOS
- **Connects to:** LND, through LNDHub

BlueWallet does not talk to LND directly. It talks to **LNDHub**, an account layer in front of a node — which makes it a reasonable choice when one node serves several people, and a poor one if you want the wallet to _be_ your node.

> [!NOTE]
> BlueWallet's hosted LNDHub service shut down in 2023; the self-hosted software still works. On StartOS the practical route is [LNbits](#lnbits), whose LNDHub extension serves BlueWallet from your own node.

- [BlueWallet website](https://bluewallet.io/)
- [LNDHub documentation](https://bluewallet.io/lndhub/)

### FullyNoded

- **Platforms:** iOS, macOS
- **Connects to:** LND, Core Lightning
- **Tor:** Integrated; everything routes through it

Primarily a Bitcoin wallet — see [Bitcoin Wallets](bitcoin-wallets.md#fullynoded) — with Lightning support alongside. Because it connects over Tor to a `.onion` address, there is no certificate step.

- [FullyNoded website](https://fullynoded.app/)
- [Lightning documentation](https://fonta1n3.github.io/FullyNoded/Docs/Lightning.html)

### Zeus

- **Platforms:** Android, iOS
- **Connects to:** LND, Core Lightning, Eclair, LNDHub
- **Tor:** Native on Android; experimental on iOS

The most capable mobile Lightning wallet, and the one with the most ways in. It can also run an embedded node on the phone itself, with no remote connection at all.

**To connect,** in Zeus go to **Settings → Connect a node → +**, choose your implementation, and take the matching credential from your server:

| Your node                        | Zeus implementation | Take from                                            |
| -------------------------------- | ------------------- | ---------------------------------------------------- |
| LND                              | LND                 | The **REST** interface's `lndconnect://` QR          |
| Core Lightning                   | CLNRest             | The **CLNrest** interface URL, rune already embedded |
| LND, without exposing an address | LNC                 | Lightning Terminal's Connect page                    |

Enable Tor in Zeus only if you are actually using a `.onion` address. For a LAN or clearnet address, leave certificate validation on and install the [StartOS Root CA](/start-os/trust-ca.html) on the phone.

- [Zeus website](https://zeusln.com/)
- [Connecting Zeus to StartOS](https://docs.zeusln.app/for-users/remote-connections/startos)

## Tools that run on your server

### Alby Hub

- **Connects to:** LND, or its own embedded LDK node

Bridges Lightning to the Nostr ecosystem through Nostr Wallet Connect (NWC), letting Nostr clients, podcasting apps, and web apps spend from your node under permissions you set per app. Also available as a desktop app if you would rather not self-host it.

- [Alby Hub website](https://albyhub.com/)
- [Setup guide](https://guides.getalby.com/user-guide/alby-hub)

### BTCPay Server

- **Connects to:** LND, Core Lightning, Eclair

A self-hosted payment processor: invoices, point-of-sale, and e-commerce plugins for WooCommerce, Shopify and others. Its Lightning integration supports all three implementations, and it also has a full on-chain wallet — see [Bitcoin Wallets](bitcoin-wallets.md#btcpay-server).

- [BTCPay Server website](https://btcpayserver.org/)
- [Lightning documentation](https://docs.btcpayserver.org/LightningNetwork/)

### CLN Application

- **Connects to:** Core Lightning only

Blockstream's official web dashboard for Core Lightning. It is part of the Core Lightning service rather than a separate install, and is reached through that service's **Web UI** interface.

- [Source code](https://github.com/ElementsProject/cln-application)

### Lightning Terminal

- **Connects to:** LND only

Lightning Labs' dashboard for LND, bundling Loop (swaps between channel and on-chain funds), Pool (a liquidity marketplace), and Taproot Assets. Its daemon also provides [Lightning Node Connect](#lightning-node-connect), which is reason enough to install it even if you prefer another dashboard: LNC is what lets Zeus reach your node without an onion address or a domain.

- [Lightning Terminal documentation](https://docs.lightning.engineering/lightning-network-tools/lightning-terminal/introduction)

### LNbits

- **Connects to:** LND, Core Lightning, Eclair

An accounts-and-extensions layer over your node. It carves out isolated wallets — useful for a household, a business, or anything you would rather not hand the admin macaroon — and its extensions cover paywalls, point-of-sale, tipping, and an LNDHub backend that serves [BlueWallet](#bluewallet) and Zeus from your own node.

- [LNbits website](https://lnbits.com/)
- [Backend wallet configuration](https://docs.lnbits.org/guide/wallets.html)

### RTL

- **Connects to:** LND, Core Lightning, Eclair

Ride The Lightning: the broadest node-management web UI, and the only one here that covers all three implementations. Channel management, payment history, routing fee configuration, and Loop/Pool integration.

- [RTL website](https://www.ridethelightning.info/)
- [Source code](https://github.com/Ride-The-Lightning/RTL)
