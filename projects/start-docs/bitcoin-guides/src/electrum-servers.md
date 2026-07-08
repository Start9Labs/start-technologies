# Electrum Servers

An Electrum server sits between your Bitcoin node and your wallet. It indexes the blockchain so wallets can quickly look up balances, transaction history, and unspent outputs for any address — without scanning the entire chain themselves. Most Bitcoin wallets connect to your self-hosted infrastructure through an Electrum server rather than directly via Bitcoin RPC.

A Bitcoin node stores every block ever produced, but it does not maintain an index of which addresses own which coins. When a wallet asks "what is the balance of this address?", the node would have to scan the entire blockchain to answer. An Electrum server solves this by building a persistent index that maps addresses to their transactions and unspent outputs. Once the index is built, lookups are instant. The server speaks the Electrum protocol, a lightweight client-server protocol that wallets already know how to use.

```
┌──────────┐      Electrum       ┌─────────────────┐       RPC        ┌──────────────┐
│  Wallet  │ ──── protocol ────▶ │ Electrum Server  │ ──────────────▶  │ Bitcoin Node  │
└──────────┘                     │ (Fulcrum)        │                  │              │
                                 │ address index    │                  │ full chain   │
                                 └─────────────────┘                  └──────────────┘
```

1. Your **Bitcoin node** downloads and validates every block on the network.
2. Your **Electrum server** reads blocks from the node and builds an address-level index.
3. Your **wallet** connects to the Electrum server and queries balances, history, and UTXOs instantly.

The initial index build requires scanning the full blockchain, which can take hours or days depending on the implementation and your hardware. After that, the server stays in sync incrementally as new blocks arrive.

## Options on StartOS

### Fulcrum (Recommended)

- **Author:** Calin Culianu (cculianu)
- **Language:** C++
- **StartOS:** Available on the StartOS Marketplace

Fulcrum is a high-performance Electrum server written in C++. It is the recommended Electrum server for StartOS.

**Why Fulcrum:**

- **Fast queries** — Once indexed, address lookups and UTXO queries are consistently fast, even for addresses with large transaction histories.
- **Full address index** — Fulcrum builds a complete index of all addresses, so every wallet query is served from local data.
- **Mature and stable** — Actively maintained, widely deployed across the Bitcoin self-hosting community.
- **Broad wallet support** — Every wallet that speaks the Electrum protocol works with Fulcrum. See [Bitcoin Wallets](bitcoin-wallets.md) for a full list.

**Trade-offs:**

- **Disk usage** — The full address index requires significant disk space (roughly 60-100 GB on top of the blockchain itself).
- **Initial sync time** — Building the index from scratch takes time, typically several hours on modern hardware. Plan to let it run overnight after installation.

### electrs

- **Author:** Blockstream
- **Language:** Rust

electrs is a lightweight Electrum server written in Rust. It takes a different approach than Fulcrum — instead of building a full persistent index, electrs uses a compact index and fetches some data from the Bitcoin node on demand.

**Advantages:**

- **Lower disk usage** — The compact index uses significantly less disk space than Fulcrum's full index.
- **Faster initial sync** — The lighter index builds faster than a full address index.

**Disadvantages:**

- **Slower queries** — Some queries require on-the-fly lookups from the Bitcoin node, making them noticeably slower than Fulcrum, especially for addresses with many transactions.
- **Not on the StartOS Marketplace** — electrs is not currently available as a service on StartOS.

## Comparison

| Feature          | Fulcrum              | electrs                    |
| ---------------- | -------------------- | -------------------------- |
| **Query speed**  | Fast (full index)    | Slower (partial on-demand) |
| **Disk usage**   | ~60-100 GB for index | ~5-10 GB for index         |
| **Initial sync** | Several hours        | Faster                     |
| **StartOS**      | Yes                  | No                         |
| **Maintenance**  | Active               | Active                     |

For StartOS users, **Fulcrum is the clear choice** — it is the only Electrum server available on the StartOS Marketplace and provides the best query performance for everyday wallet use.

## Connecting Wallets

Once Fulcrum is installed and synced on your StartOS server, wallets connect to it using an Electrum server address. Most wallets need just two pieces of information:

- **Host** — Your server's address (local IP on your LAN, `.local` hostname, `.onion` address over Tor, or a clearnet address via VPN or reverse tunnel)
- **Port** — The Electrum protocol port (typically 50001 for TCP or 50002 for SSL)

For wallet-specific connection instructions, see [Bitcoin Wallets](bitcoin-wallets.md).
