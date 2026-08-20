# Electrum Servers

An Electrum server sits between your Bitcoin node and your wallet, indexing the blockchain so wallets can look up balances, history, and unspent outputs for an address instantly. Most Bitcoin wallets reach a self-hosted node this way rather than over Bitcoin RPC.

On StartOS that server is **Fulcrum**. Once it is installed and synced, see [Connecting a Wallet](connecting-wallets.md).

## Why you need one

A Bitcoin node stores every block ever produced, but it keeps no index of which addresses own which coins. Asked "what is the balance of this address?", it would have to walk the entire chain. An Electrum server solves that by building a persistent index from addresses to their transactions and unspent outputs, and serving it over the Electrum protocol — which wallets already know how to speak.

```
┌──────────┐    Electrum     ┌──────────────────┐      RPC       ┌───────────────┐
│  Wallet  │ ─── protocol ──▶│  Electrum server │ ──── + P2P ───▶│ Bitcoin node  │
└──────────┘                 │  address index   │                │  full chain   │
                             └──────────────────┘                └───────────────┘
```

1. Your **Bitcoin node** downloads and validates every block on the network.
2. Your **Electrum server** reads blocks from the node and builds an address-level index.
3. Your **wallet** connects to the Electrum server and queries balances, history, and UTXOs instantly.

Building that index the first time means reading the whole chain, which takes hours. After that the server keeps up incrementally as blocks arrive.

## Fulcrum

Fulcrum builds a **complete** address index, so every wallet query is answered from local data. Queries stay fast even for addresses with long transaction histories, which is where lighter Electrum servers struggle. It is actively maintained and widely deployed across the self-hosting community.

The cost is disk and patience: the index runs to hundreds of gigabytes on top of the chain itself, and the first build takes many hours. Install it, then leave it overnight. Fulcrum's own instructions in StartOS carry its exact requirements and settings.

It also needs your Bitcoin node to be **archival** rather than pruned — see [Archival vs Pruned Nodes](archival-vs-pruned.md). StartOS prompts you on the Bitcoin service if anything needs changing.

Wait for Fulcrum's **Sync Progress** health check to report Synced before pointing a wallet at it. A wallet connected to a half-built index shows a partial balance, which is alarming and entirely temporary.

- [Fulcrum source and documentation](https://github.com/cculianu/Fulcrum)

## Do you need one at all?

Not if your wallet talks to Bitcoin directly. Sparrow, FullyNoded, Wasabi and BTCPay Server all connect over Bitcoin's RPC interface, with no indexer in the picture and no extra disk.

The difference shows up when you **import an existing wallet**. An Electrum server finds its history in seconds; a bare Bitcoin node has to walk the block range, which takes hours and cannot reach past a pruned node's horizon. Day-to-day use of a wallet that is already imported is much the same either way.

See [Connecting a Wallet](connecting-wallets.md#which-connection-do-you-need) for the full comparison.
