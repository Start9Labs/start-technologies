# Alternative Registries

Beyond the [default registries](default-registries.md), you can add alternative registries to your Marketplace. Custom registries are useful for organizations packaging services for their own users, developers distributing services outside the default registries, or communities curating specialized collections. To switch between registries or add new ones, click "Switch" beneath the current registry title in the sidebar.

> [!NOTE]
> Start9 does not vet, endorse, or support services from custom registries. Exercise caution and only add registries you trust.

## Known Third-Party Registries

These registries are operated by third parties, not by Start9. Nothing on them is maintained, vetted, or supported by Start9, and Start9 support cannot help with a service installed from one — take questions to the registry's operator. What each carries is listed as of this writing and can change without notice.

### registry.testnet4.info

`https://registry.testnet4.info` — Bitcoin tools and wallets. This is where Sparrow and the Bitcoin testnet services went when Start9 stopped publishing them.

- **Bitcoin Core (testnet4)** and **Fulcrum (testnet4)** — a full node and Electrum server on the testnet4 test network
- **Sparrow** and **Wasabi Wallet** — desktop wallets running in a browser-based Linux desktop (Webtop)
- **Frigate Electrum Server** — an experimental Electrum server that scans for Silent Payments
- **Am I Exposed? (modded)**, **NFC Push TX**, **AxeOS Monitor**, **Cloudflare Tunnel**, **WireGuard VPN**

### start9.mempool.guide

`https://start9.mempool.guide` — services for the chain that split from Bitcoin at BIP-110 (RDTS) and later moved to a BLAKE2b proof of work. **It is a different network from Bitcoin.**

- **Bitcoin Knots** — a node for that chain, published under the same package ID as Bitcoin Core (`bitcoind`) as a flavor of Bitcoin. Once installed, StartOS offers no way to switch it back to Bitcoin Core: returning to Bitcoin means uninstalling it, installing Bitcoin Core fresh, and syncing from the start.
- **Fulcrum (BLAKE2b)**, **Datum Gateway**, **Canary**, and **Mempool Guide** — an Electrum server, mining gateway, wallet monitor, and block explorer for the same chain.

Do not add this registry unless you intend to run that chain and understand that nothing else on your server should depend on its node.

## Hosting an Alternative Registry

Install the "StartOS Registry" service from the Marketplace and follow instructions.
