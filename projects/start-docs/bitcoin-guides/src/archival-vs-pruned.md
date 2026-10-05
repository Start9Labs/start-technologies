# Archival vs Pruned Nodes

A Bitcoin node can store the entire blockchain (archival) or only a subset of it (pruned). Both modes fully validate every block and transaction — the difference is how much historical data the node keeps on disk after validation. StartOS supports both modes and includes a feature that makes pruned nodes more capable than they are on other platforms.

## Archival Nodes

An archival node keeps every block ever produced, from the genesis block to the current tip. Nothing is deleted after validation.

**Advantages:**

- **Serves any historical block** — Other software that needs old blocks (block explorers, chain analysis tools) can fetch them directly from your node.
- **Supports block explorers** — Services like Mempool require access to the full blockchain history and only work with archival nodes.
- **Reindex without re-download** — If you need to rebuild internal databases, the node can reindex from its local copy of the chain.

**Trade-offs:**

- **Disk usage** — The full blockchain is over 600 GB and growing. You need a drive large enough to hold it plus room for continued growth.

## Pruned Nodes

A pruned node validates every block but discards block data after processing, keeping only a subset of blocks rather than the full chain. The node retains the full UTXO set (the record of all unspent coins), so it can still validate new blocks and transactions.

**Advantages:**

- **Much less disk space** — A pruned node can run with as little as 5-10 GB of block data, compared to 600+ GB for an archival node.
- **Same security** — Pruned nodes validate everything. They enforce the same consensus rules as archival nodes.

**Trade-offs:**

- **Cannot serve old blocks** — On most platforms, if a service requests a block the node has already pruned, the request fails. StartOS solves this, for services that read blocks over Bitcoin's RPC interface, with [on-demand block fetching](#pruned-nodes-on-startos).
- **Not every service is covered** — Electrum servers and block explorers still need an archival node, even on StartOS. See [What on-demand fetching does not cover](#what-on-demand-fetching-does-not-cover).

## Pruned Nodes on StartOS

On most platforms, running a pruned node introduces a practical limitation: downstream services like LND or BTCPay Server need to control which blocks are kept and which are pruned. Each service tells the node "don't prune blocks newer than X" to ensure the blocks it cares about remain available. This works fine with a single downstream service, but breaks down when multiple services each want to control pruning — they can conflict over which blocks to keep.

StartOS solves this with **on-demand block fetching**. The Bitcoin package on StartOS integrates btc-rpc-proxy, which intercepts block requests from downstream services. If a service asks for a block the pruned node has already discarded, btc-rpc-proxy fetches that block from the Bitcoin peer-to-peer network on the fly and serves it to the requesting service. From the service's perspective, it appears to be talking to an archival node.

A fetched block is not taken on trust. Before it is served, the proxy checks that it hashes to the block that was asked for, that its merkle root commits to the transactions it carries, and that its witness commitment is satisfied — so a peer can neither substitute a different block nor strip data out of one.

This means:

- **Multiple downstream services work simultaneously** — LND, Core Lightning, BTCPay Server, and any other service that reads blocks over Bitcoin's RPC interface can all connect to a single pruned node without conflict.
- **No manual pruning management** — You don't need to coordinate which blocks each service needs. btc-rpc-proxy handles it transparently.
- **Significant disk savings** — You get most of the benefits of an archival node with a fraction of the disk usage.

> [!NOTE]
>
> On-demand block fetching adds a small delay when a service requests a pruned block, since the block must be downloaded from the network before it can be served. Recently fetched blocks are held in memory, so asking for the same one again is answered without going back out to the network. In practice this is rare — most services only need recent blocks during normal operation.

### What on-demand fetching does not cover

The proxy answers over Bitcoin's RPC interface. Three things read the chain some other way, and all of them need an archival node on StartOS — each raises a task on the Bitcoin service asking you to turn pruning off.

- **Fulcrum** — it finds transactions through Bitcoin's transaction index, which records where in a block file each one sits. A pruned node cannot keep that index; Bitcoin refuses to run the two together. Fulcrum's author requires it deliberately: without it the node reads and searches an entire block for every transaction a wallet asks for, which does not hold up when several wallets are syncing at once.
- **electrs** — it pulls whole blocks over Bitcoin's peer-to-peer interface rather than its RPC, so the proxy is not in that path. Upstream has kept it that way partly on privacy grounds: on a pruned node, the blocks the server went out to fetch would track which addresses your wallet had asked about.
- **Block explorers** — Mempool answers arbitrary queries across the whole chain and needs it on disk.

> [!NOTE]
>
> A community-maintained fork of electrs, [electrs-pruned](https://github.com/paulscode/electrs-pruned), routes the reads the proxy can answer through it and does work against a pruned node. It is **not a Start9 package** — not in the marketplace, not audited by us, and installed by sideloading. It carries the privacy trade-off above, and its first index build against an already-pruned node takes over a day, considerably longer if Bitcoin reaches the network only over Tor.

## Managing Pruning on StartOS

Pruning is controlled from the Bitcoin service's Actions menu: **Services > Bitcoin > Actions > Other**. From there you can enable or disable pruning and set the prune target (the amount of block data to retain, in MB).

### Automatic pruning on small disks

If your server's total disk capacity is under 900 GB, StartOS **enables pruning by default and enforces it** — you cannot disable it. The full blockchain (600+ GB and growing) would consume too much of the available space, leaving insufficient room for other services and system operations. On these drives, pruning is not optional.

If your disk is 900 GB or larger, pruning is off by default and you can enable or disable it freely.

## Which Should You Choose?

For most StartOS users, a **pruned node** is the best choice. Thanks to on-demand block fetching, a wallet that talks to Bitcoin directly and a Lightning node beside it both work against a pruned node exactly as they would against an archival one, at a fraction of the disk usage.

Choose an **archival node** if:

- You want to run an **Electrum server** — Fulcrum or electrs. Most wallets reach a node through one, so this is the common reason to need an archival node.
- You want to run a **block explorer** like Mempool.
- You have the disk space (900 GB or more) and prefer having a complete local copy of the blockchain for its own sake.

| Feature                                | Archival            | Pruned (on StartOS)                |
| -------------------------------------- | ------------------- | ---------------------------------- |
| **Validates all blocks**               | Yes                 | Yes                                |
| **Disk usage**                         | 600+ GB and growing | 5-10 GB (configurable)             |
| **LND, Core Lightning, BTCPay**        | Yes                 | Yes (via on-demand block fetching) |
| **Multiple downstream services**       | Yes                 | Yes (via on-demand block fetching) |
| **Electrum server (Fulcrum, electrs)** | Yes                 | No                                 |
| **Block explorer (Mempool)**           | Yes                 | No                                 |
