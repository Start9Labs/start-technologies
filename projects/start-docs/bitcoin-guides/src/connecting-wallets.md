# Connecting a Wallet

Every wallet that talks to your own node needs the same four things: an address, a port, a way to trust your server's certificate, and — if you are away from home — a route in. This page covers all four once, so the per-wallet pages only have to tell you where the settings live.

Start here, then look up your wallet in [Bitcoin Wallets](bitcoin-wallets.md) or [Lightning Wallets](lightning-wallets.md).

## Which connection do you need?

Two different things on your server can serve a wallet, and which one you want depends on the wallet.

|                              | **Electrum server**                 | **Bitcoin RPC**                                                            |
| ---------------------------- | ----------------------------------- | -------------------------------------------------------------------------- |
| What it is                   | Fulcrum, indexing your Bitcoin node | Your Bitcoin node's own JSON-RPC API                                       |
| Install                      | Bitcoin **and** Fulcrum             | Bitcoin only                                                               |
| Extra disk                   | Hundreds of GB for the index        | None                                                                       |
| Importing an existing wallet | Seconds                             | Walks the whole chain — hours, and impossible past a pruned node's horizon |
| Used by                      | Most wallets                        | FullyNoded, Wasabi, BTCPay Server                                          |

Most wallets speak the Electrum protocol, so most people install [Fulcrum](electrum-servers.md) and never think about RPC. Bitcoin RPC is the right answer when a wallet only supports it, or when you want to avoid the index's disk cost and are not importing wallets with long histories.

Some wallets — Sparrow, Nunchuk, Liana — will take either. Prefer the Electrum server if you have one running: address lookups come from a purpose-built index instead of a chain scan.

## Getting the address

On your server, open your Electrum server and copy an address from its **Electrum (SSL)** interface. Bitcoin's **RPC** interface works the same way.

The address already contains the host and the port, in the form `ssl://<host>:<port>`. Copy it; do not retype it from memory.

> [!WARNING]
> **Take the port from the address.** StartOS assigns the external port when the service is installed and never changes it afterwards, so it is a property of your server rather than of the software. 50002 is a preference, not a guarantee. Wallet guides that tell you to enter 50001 or 50002 are describing a hand-configured server, not this one.

Which address you copy depends on where the wallet is:

- **LAN IP** (`192.168.x.x`) — a phone or laptop on your home network. The most reliable choice.
- **`.local` hostname** (`server-name.local`) — the same, but it survives your router handing the server a new IP. Needs mDNS, which some networks and VPNs block.
- **`.onion`** — reachable from anywhere, no port forwarding, no domain. Requires Tor at both ends.
- **Custom domain** — reachable from anywhere and faster than Tor. Requires setting one up.

See [Reaching your server from outside your home](#reaching-your-server-from-outside-your-home) below for the last two.

## Every address is `ssl://`

StartOS terminates TLS in front of the Electrum server and exposes **only** the encrypted endpoint. There is no plaintext port to connect to from off the server, on any address — LAN, `.local`, Tor, and custom domains alike.

So in your wallet, SSL/TLS has to be **on**. How you say that varies:

| Wallet expresses SSL as             | Examples                               |
| ----------------------------------- | -------------------------------------- |
| A checkbox                          | Sparrow (_Use SSL_), BlueWallet        |
| An `:s` suffix on the server string | Electrum (`host:port:s`), Trezor Suite |
| An `ssl://` prefix                  | Nunchuk                                |
| Always on, no setting               | BitBoxApp, Bitcoin Keeper              |

> [!NOTE]
> Getting this wrong rarely produces an error that says so. Sparrow reports "Retries exhausted"; most wallets just show a spinner or a red dot. If a connection fails and you have not explicitly turned SSL on, that is the first thing to check.
>
> This also means older walkthroughs no longer apply. Guides that tell you to append `:t` (plain TCP) or connect to port 50001 were written when StartOS exposed a plaintext Electrum port. It does not.

## Making your wallet trust the certificate

The certificate StartOS serves is issued by **your server's own root certificate authority** — one it generated for itself when you set it up. It is a real certificate and it is doing real work, but no wallet has heard of the authority that signed it, so every wallet has to be told to trust it once.

There are four ways that happens, and which one applies is a property of the wallet:

1. **The wallet uses your device's trust store.** Install the StartOS Root CA on the device, exactly as you did to reach the dashboard in your browser — see [Trusting Your Root CA](/start-os/trust-ca.html). Nothing further in the wallet.

2. **The wallet offers to fetch and pin the certificate.** The BitBoxApp does this: it shows a **Download remote certificate** step before it will check the connection. Accept it.

3. **The wallet keeps its own certificate store.** Sparrow has a certificate field in its server settings; Electrum reads a file you place on disk. See [The Electrum desktop wallet](#the-electrum-desktop-wallet) below.

4. **You attached a custom domain with an ACME certificate.** Then the certificate is signed by a public authority, every wallet already trusts it, and there is nothing to do.

> [!NOTE]
> Tor does not exempt you from this. A web interface reached at a `.onion` address is served over plain HTTP, because Tor already encrypts and authenticates the connection — which is why the Root CA guide says you don't need it for Tor. The Electrum interface is different: it is TLS on every address, including `.onion`, so a wallet connecting over Tor still has to trust the certificate.

## The Electrum desktop wallet

Electrum is the one wallet that needs a file placed by hand, and it is worth understanding why, because the failure is silent.

Electrum checks a server against a bundled list of public certificate authorities, and when that check fails it looks at _how_ it failed. A server presenting a **self-signed certificate** is one Electrum pins on the spot and connects to. Your server presents something different — a certificate signed by an intermediate, which is signed in turn by your server's own root authority — and that is a different failure, which Electrum treats as a server it cannot use. It never gets as far as offering to pin anything. The behaviour is tracked upstream as [spesmilo/electrum#7459](https://github.com/spesmilo/electrum/issues/7459).

The fix is to give Electrum the authority instead of the certificate. Electrum loads the file at `certs/<host>` as a trust anchor, so putting your root CA there makes the whole chain verify — and keeps working when your server renews its certificate, which pinning would not.

1. Download your root CA from `http://<your-server>/static/local-root-ca.crt` — plain HTTP, and the same file the browser guide gives you.

2. **Delete any file already sitting at `<data-dir>/certs/<host>`.** A leftover from an earlier attempt stops everything below from working and reports nothing. An empty file is the worst case: to Electrum it means "this server is publicly signed", so it goes back to the public authority list and fails again.

3. Save the certificate as `<data-dir>/certs/<host>`, **with no file extension**. `<host>` must match exactly what you type into Electrum — reaching the same server at `192.168.1.5` and at `my-server.local` needs one file under each name.

4. In Electrum, go to **Tools → Network**. Set **Connection mode** to _Connect only to a single server_ so Electrum cannot fall back to a public one, and enter the server as `<host>:<port>:s`, taking the port from the interface address.

`<data-dir>` is:

| Platform | Path                                     |
| -------- | ---------------------------------------- |
| Linux    | `~/.electrum`                            |
| macOS    | `~/Library/Application Support/Electrum` |
| Windows  | `%APPDATA%\Electrum`                     |

To connect over Tor as well, switch to the **Proxy** tab, enable the proxy, and point it at `127.0.0.1` port `9050` (or `9150` if you are using Tor Browser's).

> [!TIP]
> Do this once per address you use, not once per session. If Electrum stops connecting after a while, check whether your root CA has expired — Electrum deletes the pinned file when the certificate in it is out of date, and then falls back to failing against the public authority list.

## Reaching your server from outside your home

On your home network, the LAN IP or `.local` address is all you need. To reach your node while away, pick one:

- **Tor.** Install the **Tor** service from the marketplace, then add an onion address to the Electrum interface. No port forwarding, no domain, no static IP — but Tor is slow, and the wallet needs its own Tor connection: a SOCKS proxy on desktop (`127.0.0.1:9050`), or Orbot on mobile. Some wallets bundle their own Tor and need no proxy setup at all. See [Tor](/start-os/tor.html).

- **A custom domain.** Attach one to the interface and, if you request an ACME certificate, every wallet trusts it with no further setup. Faster and more reliable than Tor. See [Public Access](/start-os/public-access.html).

- **A VPN back to your home network.** Then the LAN address keeps working as though you were home — though `.local` names often do not survive a VPN, so use the IP. See [Remote Access](/start-os/remote-access.html).

## Connecting over Bitcoin RPC instead

A wallet that talks to Bitcoin directly needs an address from Bitcoin's **RPC** interface plus a username and password, which the Bitcoin service's **Generate RPC User Credentials** action mints for you. (Services running on the server never need this — they configure themselves.)

StartOS serves that interface over TLS on LAN and clearnet addresses with the same certificate as everything else, so the trust step above applies here too. Onion addresses are served over plain HTTP and need no certificate.

Expect wallet imports and rescans to be slow: a Bitcoin node keeps no per-address index, so finding an existing wallet's history means walking the block range. On a pruned node, history older than the prune horizon cannot be rescanned at all. Day-to-day use of a wallet that is already imported is unaffected.

## When it doesn't connect

| Symptom                                                             | Likely cause                                                                                                  |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Generic failure, no mention of TLS ("Retries exhausted", a red dot) | SSL is off in the wallet, or the port is wrong                                                                |
| Certificate, "untrusted", or "verification failed" error            | The device or wallet has not been given your root CA — see [above](#making-your-wallet-trust-the-certificate) |
| Electrum refuses the server and nothing you change helps            | A stale file at `certs/<host>` — delete it and start again                                                    |
| Worked yesterday, fails today, LAN address                          | Your router gave the server a new IP; use the `.local` name or re-copy the address                            |
| Connects but shows no balance or a partial history                  | The Electrum server has not finished indexing — check its **Sync Progress** health check                      |
| `.onion` address times out                                          | The wallet has no Tor route: no SOCKS proxy set, or Orbot is not running                                      |

If the service itself is unhealthy, start there rather than in the wallet — an Electrum server still building its index answers slowly or not at all, and that looks exactly like a connection problem.
