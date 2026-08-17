# Bitcoin Wallets

Bitcoin wallets that can be pointed at your own node — either through an Electrum server such as [Fulcrum](electrum-servers.md) or directly over Bitcoin RPC. Each entry gives the platforms, the connection method, and where the setting lives in that wallet.

Read [Connecting a Wallet](connecting-wallets.md) first: the address, the port, the SSL requirement, and the certificate step are the same for every wallet on this page, and are not repeated in each entry. For Lightning, see [Lightning Wallets](lightning-wallets.md).

## Summary

### Apps you run on your own devices

| Wallet                              | Platforms                           | Connects to     | Tor            |
| ----------------------------------- | ----------------------------------- | --------------- | -------------- |
| [BitBoxApp](#bitboxapp)             | Android, iOS, Linux, macOS, Windows | Electrum server | Yes            |
| [Bitcoin Keeper](#bitcoin-keeper)   | Android, iOS                        | Electrum server | Yes            |
| [Blockstream App](#blockstream-app) | Android, iOS, Linux, macOS, Windows | Electrum server | Yes            |
| [BlueWallet](#bluewallet)           | Android, iOS                        | Electrum server | Via Orbot      |
| [Bull Wallet](#bull-wallet)         | Android, iOS                        | Electrum server | Yes            |
| [Electrum](#electrum)               | Android, Linux, macOS, Windows      | Electrum server | Yes            |
| [Envoy](#envoy)                     | Android, iOS                        | Electrum server | Yes            |
| [FullyNoded](#fullynoded)           | iOS, macOS                          | Bitcoin RPC     | Yes            |
| [Liana](#liana)                     | Linux, macOS, Windows               | Both            | External proxy |
| [Nunchuk](#nunchuk)                 | Android, iOS, Linux, macOS, Windows | Both            | Yes            |
| [Sparrow](#sparrow)                 | Linux, macOS, Windows               | Both            | Built-in       |
| [Trezor Suite](#trezor-suite)       | Android, iOS, Linux, macOS, Windows | Electrum server | Yes            |
| [Wasabi](#wasabi)                   | Linux, macOS, Windows               | Bitcoin RPC     | Built-in       |

### Wallets that run on your server

These install from the StartOS Marketplace and reach Bitcoin over the server's internal network, so there is no address, port, or certificate to configure — and no wallet traffic leaving your house at all. Each one's own instructions in StartOS cover its setup.

| Wallet                          | Connects to |
| ------------------------------- | ----------- |
| [BTCPay Server](#btcpay-server) | Bitcoin RPC |
| [Jam](#jam)                     | Bitcoin RPC |

## Apps you run on your own devices

### BitBoxApp

- **Platforms:** Android, iOS, Linux, macOS, Windows
- **Connects to:** Electrum server
- **Certificate:** Handled in-app — it offers to fetch your server's certificate

The companion app for BitBox hardware wallets, and usable on its own. Of the wallets here it has the smoothest path to a private server: it fetches and trusts the certificate itself, with no OS-level or file-level step.

**To connect:**

1. Go to **Settings → Advanced settings → Connect your own full node**.
2. Paste the host and port from your **Electrum (SSL)** interface address as `host:port`. Upstream warns not to substitute 50001 or 50002 for the port your node reports — on StartOS that warning is exactly right.
3. Choose **Download remote certificate** when prompted, then **Check** to test, then **Add**.

For Tor, enable the Tor proxy under **Advanced settings** first (`127.0.0.1:9050`, or `127.0.0.1:9150` for Tor Browser's), restart the app fully, then add the `.onion` endpoint. If the check fails immediately after enabling Tor, the proxy has not finished connecting — restart and retry.

- [BitBoxApp downloads](https://bitbox.swiss/download/)
- [Connecting to your own full node](https://support.bitbox.swiss/connecting-the-bitboxapp-to-your-own-full-node)

### Bitcoin Keeper

- **Platforms:** Android, iOS
- **Connects to:** Electrum server

An open-source mobile wallet built around multisig and inheritance, with an Electrum server management screen for pointing it at a server of your own. Add your **Electrum (SSL)** address there; Keeper requires SSL, which is what StartOS serves.

- [Bitcoin Keeper website](https://bitcoinkeeper.app/)
- [Source code](https://github.com/bithyve/bitcoin-keeper)

### Blockstream App

- **Platforms:** Android, iOS, Linux, macOS, Windows
- **Connects to:** Electrum server

Formerly Blockstream Green. It supports a personal Electrum server and has a built-in Tor toggle, so no separate proxy is needed.

**To connect (desktop):**

1. Open **App Settings** (the gear icon) and choose **Network**.
2. Turn on **Connect with Tor** if you are using a `.onion` address.
3. Go back, then open **Custom servers and validation** and turn on **Choose the Electrum servers you trust**.
4. Paste your address into **Bitcoin Electrum Server**.

**To connect (mobile):** open **App Settings**, turn on **Connect with Tor** if needed, turn on **Personal Electrum Server**, paste the address into **Bitcoin Electrum server**, and **Save**.

> [!NOTE]
> Recent versions expose a TLS toggle for personal Electrum servers — leave it on, since StartOS serves TLS only. Whether the app will accept a certificate signed by your server's own authority has not been verified against a StartOS server; if it refuses, a custom domain with an ACME certificate sidesteps the question entirely.

- [Blockstream App downloads](https://blockstream.com/app/)

### BlueWallet

- **Platforms:** Android, iOS
- **Connects to:** Electrum server

A popular and approachable mobile wallet. Set the server under **Settings → Network → Electrum server** — either type the host and port, or use **Scan or import a file** to scan the QR code from your Electrum interface. Save, then restart the app.

> [!WARNING]
> BlueWallet over Tor has a long history of connecting once and then dropping, with no error that explains why. Orbot must be installed and running before it will work at all, and even then StartOS users report repeat disconnections. If you are connecting from outside your home and BlueWallet will not stay up, that is a known rough edge rather than something misconfigured on your server — Sparrow and Nunchuk are the usual fallbacks.

- [BlueWallet website](https://bluewallet.io/)
- [Electrum servers pool](https://github.com/BlueWallet/BlueWallet/wiki/Electrum-servers-pool)

### Bull Wallet

- **Platforms:** Android, iOS
- **Connects to:** Electrum server

Bull Bitcoin's mobile wallet, combining on-chain Bitcoin, Lightning (via Boltz swaps), and Liquid. It supports a custom Electrum server and works as a companion for hardware wallets, with watch-only imports and air-gapped signing for the Coldcard Q.

- [Bull Wallet website](https://www.bullbitcoin.com/blog/bull-by-bull-bitcoin)
- [Source code](https://github.com/SatoshiPortal/bullbitcoin-mobile)

### Electrum

- **Platforms:** Android, Linux, macOS, Windows
- **Connects to:** Electrum server
- **Certificate:** Needs a file placed by hand

The original Electrum-protocol wallet, and the one wallet here that will not connect to a StartOS server until you give it your root CA on disk. The reason and the full procedure are on [Connecting a Wallet](connecting-wallets.md#the-electrum-desktop-wallet) — do that first, then set the server under **Tools → Network** as `<host>:<port>:s` with **Connection mode** set to _Connect only to a single server_.

- [Electrum downloads](https://electrum.org/#download)
- [Using Electrum through Tor](https://electrum.readthedocs.io/en/latest/tor.html)

### Envoy

- **Platforms:** Android, iOS
- **Connects to:** Electrum server

The companion app for Foundation's Passport hardware wallet, also usable standalone, with a built-in Tor toggle. Add your **Electrum (SSL)** address in Envoy's settings; a red shield in the app means it cannot reach the custom server you configured.

- [Envoy downloads](https://foundation.xyz/envoy/)
- [Envoy settings](https://docs.foundation.xyz/envoy/envoy-menu/settings/)

### FullyNoded

- **Platforms:** iOS, macOS
- **Connects to:** Bitcoin RPC
- **Certificate:** None needed over Tor

Talks to Bitcoin directly over its RPC interface, with Tor integrated — no Electrum server involved. Because it connects to a `.onion` address, which StartOS serves without TLS, there is no certificate step.

**To connect:** run **Generate RPC User Credentials** on the Bitcoin service, then add a node in FullyNoded with Bitcoin's `.onion` RPC address and those credentials.

- [FullyNoded website](https://fullynoded.app/)
- [Connect your node](https://fonta1n3.github.io/FullyNoded/Docs/Bitcoin-Core/Connect.html)

### Liana

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Electrum server or Bitcoin RPC

Built around timelocked recovery and inheritance — spending paths that unlock after a period of inactivity. It takes either backend, and will install a pruned node of its own if you have none; on StartOS, point it at the node you already run instead. Tor needs an external proxy.

- [Liana website](https://wizardsardine.com/liana/)
- [Source code](https://github.com/wizardsardine/liana)

### Nunchuk

- **Platforms:** Android, iOS, Linux, macOS, Windows
- **Connects to:** Electrum server or Bitcoin RPC

A multisig- and inheritance-focused wallet on every major platform, and one of the more reliable choices for a private server.

**To connect:** open **Network Settings** — under your profile on mobile, and on desktop under **Settings → Network settings**, reached from your profile picture — then put your address in **Mainnet server**, keeping the `ssl://` prefix:

```
ssl://adjective-noun.local:50002
```

For a `.onion` address, also turn on **Enable TOR proxy** on the same screen, and make sure Tor is running on the device. Save and restart Nunchuk.

- [Nunchuk website](https://nunchuk.io/)
- [Source code](https://github.com/nunchuk-io/nunchuk-desktop)

### Sparrow

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Electrum server or Bitcoin RPC
- **Certificate:** Accepts one, or takes a `.crt` file in its server settings
- **Tor:** Bundled — `.onion` addresses work with no proxy setup

The power-user desktop wallet, and the least painful of the desktop options to point at a StartOS server: it will connect to an unrecognised certificate, and it routes `.onion` addresses through its own Tor daemon automatically.

**To connect:**

1. Go to **File → Preferences → Server** (on first run Sparrow takes you straight there).
2. Choose **Private Electrum**.
3. Enter the host in **URL** and the port from your Electrum interface address.
4. Turn **Use SSL** on.
5. Click **Test Connection**.

Optionally supply your root CA in the certificate field on the same screen. Sparrow will connect without it, but pinning the authority means a swapped certificate is caught rather than silently accepted.

If you are using your own Tor daemon rather than Sparrow's — or routing through a proxy for another reason — enable **Use Proxy** with `localhost` and port `9050`. Leave it off otherwise.

To use Bitcoin RPC instead, choose **Bitcoin Core** on the same screen and supply the RPC address with the credentials from **Generate RPC User Credentials**.

- [Sparrow downloads](https://sparrowwallet.com/download/)
- [Connect to Bitcoin Core](https://sparrowwallet.com/docs/connect-node.html)

### Trezor Suite

- **Platforms:** Linux, macOS, Windows (desktop), Android, iOS (mobile)
- **Connects to:** Electrum server

The companion software for Trezor hardware wallets. A custom Electrum backend replaces Trezor's own servers, so your addresses stop being queried against them.

**To connect:** go to **Settings → Networks**, click the sliders icon next to Bitcoin, and enter your server in `host:port:protocol` form with `s` for SSL:

```
adjective-noun.local:50002:s
```

For a `.onion` address, turn on Tor in Trezor Suite first — it will prompt you if you have not.

- [Trezor Suite downloads](https://trezor.io/trezor-suite)
- [Full node via Electrum server](https://trezor.io/learn/supported-assets/bitcoin/full-node-via-electrum-server)

### Wasabi

- **Platforms:** Linux, macOS, Windows
- **Connects to:** Bitcoin RPC
- **Tor:** Built-in and on by default for all traffic

A privacy-focused desktop wallet built around coinjoin. It finds your transactions using BIP158 block filters rather than an address index, so it wants Bitcoin's RPC rather than an Electrum server — and it needs block filters turned on, which is an option under Bitcoin's **Other Settings**.

- [Wasabi downloads](https://wasabiwallet.io/)
- [Using a Bitcoin full node](https://docs.wasabiwallet.io/using-wasabi/BitcoinFullNode.html)

## Wallets that run on your server

### BTCPay Server

- **Connects to:** Bitcoin RPC

A self-hosted payment processor with a capable on-chain wallet attached: hot wallets, watch-only wallets from an xpub with PSBT signing elsewhere, coin selection, and payment batching. Install it and it wires itself to Bitcoin — there is nothing to configure.

BTCPay also handles Lightning; see [Lightning Wallets](lightning-wallets.md#btcpay-server).

- [BTCPay Server website](https://btcpayserver.org/)
- [Wallet documentation](https://docs.btcpayserver.org/Wallet/)

### Jam

- **Connects to:** Bitcoin RPC

A web interface for JoinMarket: collaborative transactions that break the common-input heuristic, and the option to earn fees by offering liquidity to other traders. Tor runs inside it, so market connectivity needs no setup.

Jam imports its wallet into Bitcoin and rescans the chain, so it needs an archival node — see [Archival vs Pruned Nodes](archival-vs-pruned.md).

- [Jam documentation](https://jamdocs.org/)
