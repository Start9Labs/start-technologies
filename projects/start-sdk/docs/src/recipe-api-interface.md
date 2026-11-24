# Expose an API-Only Interface

Some services have no web UI — they expose only a programmatic API (REST, gRPC, or custom protocol). The URL is shown as a copyable connection string rather than a clickable browser link.

## Solution

Same as a web UI but use `type: 'api'` and `masked: true` on the interface. An exported interface is on the LAN by default, so before exporting an API confirm it demands a credential: never pass upstream's auth-off flag (`--disable-rpc-login`) or widen a listener upstream defaults to localhost in order to export it. If upstream has no auth, gate the binding with [`proxy.auth`](interfaces.md#authenticating-at-the-proxy); if only dependents use the API, don't export it at all ([bridge-only](service-to-service.md#exposing-a-bridge-only-port)). This shows the URL as a copyable connection string rather than a clickable browser link. For custom protocol schemes (e.g., `lndconnect://`, `smp://`), set `schemeOverride: { ssl: 'custom-scheme', noSsl: 'custom-scheme' }`.

**Reference:** [Interfaces](interfaces.md)

## Examples

See `startos/interfaces.ts` in: [ollama](https://github.com/Start9Labs/ollama-startos), [phoenixd](https://github.com/Start9Labs/phoenixd-startos), [simplex](https://github.com/Start9Labs/simplex-startos) (SMP + XFTP with custom schemes), [lnd](https://github.com/Start9Labs/lnd-startos) (lndconnect:// URIs)
