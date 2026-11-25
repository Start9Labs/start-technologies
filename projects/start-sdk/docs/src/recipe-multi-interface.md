# Expose Multiple Interfaces

Services often need more than a web UI — RPC endpoints, peer-to-peer connections, WebSocket servers, SSH, or admin dashboards on separate ports. Each interface gets its own `MultiHost`, port binding, and interface descriptor.

## Solution

In `setupInterfaces()`, create separate `MultiHost` instances for each interface (web UI, API, peer). Each gets its own `bindPort()` call with appropriate protocol settings — `protocol: 'http'` for web, `protocol: 'https'` with `addSsl` for APIs, `protocol: null` with `secure: { ssl: false }` for raw TCP. Create interfaces with `type: 'ui'`, `type: 'api'`, or `type: 'p2p'` as appropriate. Use `masked: true` for interfaces whose URLs contain credentials.

**Reference:** [Interfaces](interfaces.md)

## Examples

See `startos/interfaces.ts` in: [bitcoin-core](https://github.com/Start9Labs/bitcoin-core-startos) (RPC, peer, I2P), [cln](https://github.com/Start9Labs/cln-startos) (Web, RPC, peer, gRPC, CLNrest, WebSocket, Watchtower), [lnd](https://github.com/Start9Labs/lnd-startos) (REST, gRPC, peer, Watchtower), [monerod](https://github.com/Start9Labs/monerod-startos) (peer, RPC, wallet-RPC), [simplex](https://github.com/Start9Labs/simplex-startos) (SMP, XFTP), [garage](https://github.com/Start9Labs/garage-startos) (S3 API, S3 Web, Admin API)
