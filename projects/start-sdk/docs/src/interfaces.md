# Interfaces

`setupInterfaces()` defines the network interfaces your service exposes and how they are made available to the user. This function runs on service install, update, and config save.

## Network Reachability

Your package declares _what_ it exposes. The **user** decides _where_ it is reachable. An interface is bound to the server's [gateways](/start-os/gateways.html), and the user enables or disables each resulting address individually from the service's **Interfaces** tab. LAN addresses (the `.local` hostname, the LAN IP) are enabled by default; public IPv4 addresses are **off** by default.

**A public domain belongs to the host, but is enabled per binding.** The user adds it naming one internal port, and its addresses — the plain one and, where the binding has `addSsl`, the TLS one — are enabled on **that** binding straight away. Every other binding on the same `MultiHost` also gains the domain, but **off by default**, to be switched on individually like any other address. So a host that binds two ports needs the domain enabled twice, and a package that starts binding a second port later does not inherit the user's earlier choice for it.

Two consequences worth internalizing before you write any interface code:

- **`type` is a label, not a control.** `'ui'`, `'api'`, and `'p2p'` tell the user what an interface is _for_. They do not select a transport, grant public access, or imply anything about how the interface is reached.
- **Tor is opt-in and per-interface.** Tor is not part of StartOS. The user installs the **Tor** service from the marketplace, and then explicitly adds an onion address to each interface they want on Tor — see [Tor](/start-os/tor.html). Nothing your package does provisions one.

> [!WARNING]
> Never state — in `README.md`, `instructions.md`, a comment, or a plan — that a service "is exposed on Tor" or "is published to the internet." Your package cannot know: no binding type, and no value of `type`, causes an onion or a clearnet address to exist. Describe what the interface serves and let the user decide how to reach it.

## Single Interface

For a service with one web interface:

```typescript
import { i18n } from './i18n'
import { sdk } from './sdk'

export const setInterfaces = sdk.setupInterfaces(async ({ effects }) => {
  const multi = sdk.MultiHost.of(effects, 'ui')
  const origin = await multi.bindPort(80, {
    protocol: 'http',
    preferredExternalPort: 80,
  })

  const ui = sdk.createInterface(effects, {
    name: i18n('Web Interface'),
    id: 'ui',
    description: i18n('The main web interface'),
    type: 'ui',
    masked: false,
    schemeOverride: null,
    username: null,
    path: '',
    query: {},
  })

  return [await origin.export([ui])]
})
```

## Multiple Interfaces

Expose multiple paths (e.g., web UI and admin panel) from the same port:

```typescript
export const setInterfaces = sdk.setupInterfaces(async ({ effects }) => {
  const multi = sdk.MultiHost.of(effects, 'web')
  const origin = await multi.bindPort(80, {
    protocol: 'http',
    preferredExternalPort: 80,
  })

  const ui = sdk.createInterface(effects, {
    name: i18n('Web UI'),
    id: 'ui',
    description: i18n('The web interface'),
    type: 'ui',
    masked: false,
    schemeOverride: null,
    username: null,
    path: '',
    query: {},
  })

  const admin = sdk.createInterface(effects, {
    name: i18n('Admin Panel'),
    id: 'admin',
    description: i18n('Admin interface'),
    type: 'ui',
    masked: false,
    schemeOverride: null,
    username: null,
    path: '/admin/',
    query: {},
  })

  return [await origin.export([ui, admin])]
})
```

Expose interfaces on separate ports:

```typescript
export const setInterfaces = sdk.setupInterfaces(async ({ effects }) => {
  const receipts = []

  // Web UI — HTTP
  const uiMulti = sdk.MultiHost.of(effects, 'ui')
  const uiOrigin = await uiMulti.bindPort(80, {
    protocol: 'http',
    preferredExternalPort: 80,
  })
  const ui = sdk.createInterface(effects, {
    name: i18n('Web Interface'),
    id: 'ui',
    description: i18n('The main browser interface'),
    type: 'ui',
    masked: false,
    schemeOverride: null,
    username: null,
    path: '',
    query: {},
  })
  receipts.push(await uiOrigin.export([ui]))

  // API — HTTPS with SSL termination
  const apiMulti = sdk.MultiHost.of(effects, 'api')
  const apiOrigin = await apiMulti.bindPort(8080, {
    protocol: 'https',
    preferredExternalPort: 8080,
    addSsl: {
      alpn: null,
      preferredExternalPort: 8080,
      addXForwardedHeaders: false,
    },
  })
  const api = sdk.createInterface(effects, {
    name: i18n('REST API'),
    id: 'api',
    description: i18n('Programmatic access'),
    type: 'api',
    masked: true,
    schemeOverride: null,
    username: null,
    path: '',
    query: {},
  })
  receipts.push(await apiOrigin.export([api]))

  // Peer — raw TCP (not HTTP)
  const peerMulti = sdk.MultiHost.of(effects, 'peer')
  const peerOrigin = await peerMulti.bindPort(9735, {
    protocol: null,
    addSsl: null,
    preferredExternalPort: 9735,
    secure: { ssl: false },
  })
  const peer = sdk.createInterface(effects, {
    name: i18n('Peer Interface'),
    id: 'peer',
    description: i18n('Peer-to-peer network connections'),
    type: 'p2p',
    masked: true,
    schemeOverride: null,
    username: null,
    path: '',
    query: {},
  })
  receipts.push(await peerOrigin.export([peer]))

  return receipts
})
```

The key steps are:

1. Create a `MultiHost` and bind a port with protocol and options
2. Create one or more interfaces using `sdk.createInterface()`
3. Export the interfaces from the origin and return the receipt(s)

## bindPort Options

| Option                          | Type                                                  | Description                                                                                                                                                                                                                 |
| ------------------------------- | ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `protocol`                      | `'http'` \| `'https'` \| `null`                       | The protocol. Use `null` for raw TCP (non-HTTP).                                                                                                                                                                            |
| `preferredExternalPort`         | `number`                                              | The port users will see in their URLs.                                                                                                                                                                                      |
| `addSsl`                        | `object` \| `null`                                    | SSL termination options for HTTPS. Set to `null` for no SSL.                                                                                                                                                                |
| `addSsl.alpn`                   | `string[]` \| `null`                                  | The ALPN protocols StartOS answers a client with, from those it asked for. `null`, the usual choice, answers with whatever it asked for.                                                                                    |
| `addSsl.preferredExternalPort`  | `number`                                              | External port for SSL connections.                                                                                                                                                                                          |
| `addSsl.addXForwardedHeaders`   | `boolean`                                             | Whether to add `X-Forwarded-*` headers.                                                                                                                                                                                     |
| `addSsl.auth`                   | `ProxyAuth` \| `null`                                 | Optional auth gate enforced by the OS reverse proxy. See [Authenticating at the Proxy](#authenticating-at-the-proxy).                                                                                                       |
| `addSsl.upstreamCertValidation` | `'disable'` \| `{ certificate: string }` \| _omitted_ | How the OS validates your container's TLS cert when it [rewraps SSL](#rewrapping-ssl-to-a-tls-container). Omit to validate against the StartOS root CA (default). See [Rewrapping SSL](#rewrapping-ssl-to-a-tls-container). |
| `secure`                        | `{ ssl: boolean }` \| `null`                          | For non-HTTP protocols, whether the connection is secure. `{ ssl: true }` with `addSsl: null` serves your container's own TLS end to end — see [Serving Your Own TLS](#serving-your-own-tls-passthrough).                   |

An `addSsl` binding on any port can carry a Let's Encrypt certificate — issuance is per name, not per port. The user's side of that is one extra requirement: Let's Encrypt validates on port `443` whatever port you bind, so StartOS asks their gateway to route `443` for the domain as well. On a gateway that cannot do it automatically they forward `443` by hand. Worth a line in your instructions for an interface on a non-standard port that users will reach from software validating against public roots — an Electrum client, say.

## Interface Options

```typescript
sdk.createInterface(effects, {
  name: i18n('Display Name'), // Shown in UI (wrap with i18n)
  id: 'unique-id', // How you find this interface under its host
  description: i18n('Description'), // Shown in UI (wrap with i18n)
  type: 'ui', // 'ui', 'api', or 'p2p'
  masked: false, // Hide URLs with sensitive credentials?
  schemeOverride: null, // Override URL scheme (see below)
  username: null, // Auth username embedded in URL
  path: '/some/path/', // URL path
  query: {}, // URL query params
})
```

| Option           | Type                                                       | Description                                                                                                                                                                    |
| ---------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `name`           | `string`                                                   | Display name shown to the user. Wrap with `i18n()`.                                                                                                                            |
| `id`             | `string`                                                   | Unique identifier. How you find this interface at runtime, by walking the host from `sdk.host.getOwn()` (see [main.ts](./main.md)).                                            |
| `description`    | `string`                                                   | Description shown to the user. Wrap with `i18n()`.                                                                                                                             |
| `type`           | `'ui'`, `'api'`, or `'p2p'`                                | `'ui'` for browser interfaces, `'api'` for programmatic endpoints, `'p2p'` for peer-to-peer connections.                                                                       |
| `masked`         | `boolean`                                                  | If `true`, the interface URL is shown as a copyable secret. Use for URLs containing credentials or tokens.                                                                     |
| `schemeOverride` | `{ ssl: string \| null; noSsl: string \| null }` \| `null` | Override the URL scheme for custom protocols. For example, `{ ssl: 'lndconnect', noSsl: 'lndconnect' }` produces `lndconnect://` URLs. Use `null` for standard `http`/`https`. |
| `username`       | `string` \| `null`                                         | Username embedded in the URL (e.g., for `smp://fingerprint:password@host`).                                                                                                    |
| `path`           | `string`                                                   | URL path appended to the base address (e.g., `'/admin/'`).                                                                                                                     |
| `query`          | `object`                                                   | URL query parameters as key-value pairs (e.g., `{ macaroon: 'abc123' }`).                                                                                                      |

> [!TIP]
> The `id` you assign to an interface is what you use in `main.ts` to retrieve hostnames for it. Interfaces are reached through their **host**: `sdk.host.getOwn(effects, hostId)` returns the host, and the interface lives at `host.bindings[internalPort].interfaces[id]`. See [Main](./main.md#getting-hostnames) for details.

## Port Ranges

Some services need a **contiguous block of ports** rather than a single one — coturn / RTP media relays, bitcoin's ZMQ notification endpoints, passive-FTP data ports. Use `bindPortRange` instead of one `bindPort` per port:

```typescript
export const setInterfaces = sdk.setupInterfaces(async ({ effects }) => {
  const turn = sdk.MultiHost.of(effects, 'turn')
  const range = await turn.bindPortRange({
    internalStartPort: 49152,
    externalStartPort: 49152, // may differ; the forward maps by offset
    numberOfPorts: 100, // 2–500 contiguous ports
  })

  await range.export(
    sdk.createRangeInterface(effects, {
      id: 'turn-relay',
      name: i18n('TURN Relay'),
      description: i18n('WebRTC media relay ports'),
    }),
  )
  return []
})
```

A range binds **TCP + UDP** together and exposes **exactly one** `api` service interface spanning the whole range. The interface is deliberately restricted compared to `createInterface`: it is always `type: 'api'` and has **no** `masked`, `username`, `path`, `query`, or `schemeOverride`. The one extra option is an optional `scheme` — a transport prefix for protocols addressed as `scheme://host:port`, e.g. `tcp` for bitcoin ZMQ:

```typescript
const zmq = sdk.MultiHost.of(effects, 'zmq')
const zmqRange = await zmq.bindPortRange({
  internalStartPort: 28332,
  externalStartPort: 28332,
  numberOfPorts: 2,
})
await zmqRange.export(
  sdk.createRangeInterface(effects, {
    id: 'zmq',
    name: i18n('ZMQ'),
    description: i18n('Bitcoin ZMQ notification endpoints'),
    scheme: 'tcp', // omit for raw UDP/TCP ranges (coturn, RTP, FTP data)
  }),
)
```

Two distinct endpoints are two `bindPortRange` calls — a range is a homogeneous pool of ports, so it maps to one named interface. Range interfaces show up in the service's **Interfaces** page using the same per-gateway address cards as single-port interfaces (non-SSL, IPv4-only). The public/WAN address is disabled by default; enabling it surfaces the exact port range to forward on the router.

Each internal port a host currently binds belongs to one claim. A `bindPortRange` covering a port the same host also passes to `bindPort` — or to another range — is rejected, because the two claims describe the same container socket under different exposure rules. Only what your package declares in the current pass counts, so folding existing single ports into a range works as long as you drop their `bindPort` calls in the same release; the disabled leftovers do not conflict, and [Retiring a Host or Binding](#retiring-a-host-or-binding) is how you give their port numbers back.

| `createRangeInterface` option | Type               | Description                                                            |
| ----------------------------- | ------------------ | ---------------------------------------------------------------------- |
| `id`                          | `string`           | Unique identifier for the range interface.                             |
| `name`                        | `string`           | Display name shown to the user. Wrap with `i18n()`.                    |
| `description`                 | `string`           | Description shown to the user. Wrap with `i18n()`.                     |
| `scheme`                      | `string` \| `null` | Optional transport prefix (e.g. `'tcp'`). Omit for raw UDP/TCP ranges. |

## Retiring a Host or Binding

`setupInterfaces()` ends every pass by **disabling** each binding it did not just declare — it does not delete it. Disabling is the right default: it keeps the row, the external port number, and the user's per-address choices, so a binding your package declares conditionally comes back at the same address they already bookmarked.

The cost is that a binding you stop declaring **for good** stays behind. It keeps its external port claimed for as long as your service is installed, it keeps recomputing its addresses, and a dependency resolving it through `getBridgeAddress` still gets a `10.0.3.1:<port>` that nothing listens on. Retire it explicitly:

```typescript
await sdk.MultiHost.of(effects, 'ui-multi').retire() // the whole host
await sdk.MultiHost.of(effects, 'api').retirePort(9090) // one port, or one range
```

`retire()` removes the host and everything under it: its bindings and port ranges, their exported service interfaces, the user's public and private domains for that host, and their per-address enable/disable and WAN opt-in choices. `retirePort()` removes whichever of the single port and the port range is bound at that `internalPort` — and both, if both are — leaving the host and its domains in place. Both return the external ports to the server's pool. Both are irreversible: after `retire()`, binding the id again starts a fresh host with none of the user's setup.

Note what that last part means: `retire()` discards configuration the **user** created, not just your package's. A domain they attached to the host goes with it, and nothing tells them. Name the host in your release notes whenever a release retires one, so they know to reattach the domain to a current interface.

### The migration pattern

Retire in the `up()` of the version that stops binding, in the same release as the `interfaces.ts` change:

```typescript
export const v2_0_0 = VersionInfo.of({
  version: '2.0.0:0',
  releaseNotes: {
    en_US: 'Upstream 2.0. The web UI moved to a single host and the bundled metrics listener was removed.',
  },
  migrations: {
    up: async ({ effects }) => {
      await sdk.MultiHost.of(effects, 'ui-multi').retire()
      await sdk.MultiHost.of(effects, 'api').retirePort(9090)
    },
    down: IMPOSSIBLE,
  },
})
```

Both halves ship together. Retiring an id your `setupInterfaces` still binds simply recreates it on the next pass, minus the user's domains — so the retire has to land in the release that drops the binding, not before or after it. `down` is `IMPOSSIBLE` because a downgrade cannot give the user their domains back.

Retiring from **inside** the `sdk.setupInterfaces` callback throws. That pass ends with the disable sweep, so a retire in the middle of it would depend on statement order.

### Why this cannot be automatic

StartOS cannot infer it. A binding missing from one pass is indistinguishable from a binding the service will declare on the next one — under a different config, a backend the user has not selected yet, or a feature they toggled off. Deleting on absence would free the external port and drop their WAN opt-in every time they turned a feature off, and hand that port number to another package before they turned it back on. That is exactly what disabling exists to prevent.

The SDK cannot infer it either: it sees only the calls a pass actually made. Only the author knows a port is gone for good, and only knows it at a version boundary — which is what a migration is.

This is the same shape as [retiring a replay key](tasks.md#retiring-a-replay-key): state your package created, that outlives the release which stopped creating it, and that only your package can say is finished.

### Failure modes

- **Retiring an id you still bind.** Migrations run before `setupInterfaces`, so the port is normally reclaimed on the same pass and nothing looks wrong. The symptom is the user's setup silently reset — a custom domain and WAN toggle back to defaults after an update.
- **A port that moved rather than disappeared.** Retiring the old binding and adding the new one in the same release keeps the host's domains, but StartOS isolates a **public** domain from a binding added after it, so the user has to re-enable that domain on the new binding. Private domains carry over on their own. Say so in your release notes.
- **Treating `false` as failure.** Both calls resolve `false` when there was nothing to remove — the normal result on a re-run, and on a server that skipped the version. Not an error.
- **Retiring the last binding on a host.** That does not retire the host. Its domains stay, now addressing nothing. Use `retire()` when the host itself is going away.

### Cleaning up after the fact

A package that already shipped a version dropping a host or a port still has the row and the port claim sitting on every server that installed it. Retire is a no-op where the id was never present, so one maintenance release naming the stale ids in its `up()` covers the whole installed base at once. List the ids in your release notes: any domain the user attached to a host you retire is removed with it, and they will want to know where to reattach it.

## TLS Termination

StartOS terminates TLS at the platform edge and proxies plain HTTP to your container. This has two important consequences any time your service generates URLs or makes scheme decisions:

**1. Inside the container, every request arrives over HTTP.** A reverse proxy like nginx will see `$scheme == "http"`, the `X-Forwarded-Proto` header is not authoritative by default, and there is no TLS certificate to terminate. Do not configure in-container HTTPS — StartOS is already doing it.

**2. The browser loaded the page over `https://`.** Any URL your service emits for the browser to consume (login redirects, API endpoints in a `config.json`, OAuth callbacks, absolute links in HTML) must use `https://`. If you emit `http://` or derive the scheme from `$scheme`, the browser will block the request as [mixed active content](https://developer.mozilla.org/en-US/docs/Web/Security/Mixed_content).

**Hardcode `https://` for browser-facing URLs** rather than interpolating `$scheme` or reading the protocol from the incoming request:

```nginx
# BAD — $scheme is always "http" inside the container
return 200 '{"api_url":"$scheme://$host/api"}';

# GOOD — match what the browser actually sees
return 200 '{"api_url":"https://$host/api"}';
```

This applies to any configuration file generated in `setupMain` or any runtime response that includes absolute URLs — not just nginx. When in doubt, hardcode `https://`.

### Application protocols

`addSsl.alpn` is the list of protocols StartOS will answer a client with, chosen from the ones that client asked for. Unset — the usual case — it answers with whatever the client asked for. A client left with nothing is refused; a client that asks for no protocol at all is served as it would be without ALPN.

StartOS answers from that list itself, because it terminates the client's TLS and forwards plain HTTP. Your container never sees the negotiation, so the list is the only thing holding a client to a protocol your container can serve — which is why `protocol: 'http'` and `'ws'` set it to `http/1.1`.

A container serving its own TLS does get a say; see [Rewrapping SSL to a TLS container](#rewrapping-ssl-to-a-tls-container).

## Rewrapping SSL to a TLS container

The guidance above ("do not configure in-container HTTPS") applies when StartOS terminates TLS and forwards plain HTTP — the `http`/`ws` protocols. The `https`/`wss` protocols are different: the container serves its **own** TLS, StartOS terminates the client's TLS at the edge, and then opens a **fresh TLS connection to your container** (a "rewrap"). This happens whenever `addSsl` is set and the protocol's `secure.ssl` is `true`.

On that inner OS→container leg, StartOS validates your container's certificate. By default it requires a certificate signed by the StartOS root CA. A container serving a **self-signed** certificate on the internal bridge will fail that check, so use `addSsl.upstreamCertValidation` to control it:

| Value                      | Behavior                                                                                                 |
| -------------------------- | -------------------------------------------------------------------------------------------------------- |
| _omitted_                  | Validate against the StartOS root CA (default).                                                          |
| `'disable'`                | Skip certificate validation entirely. Appropriate for a self-signed cert on the trusted internal bridge. |
| `{ certificate: '<pem>' }` | Validate against the supplied PEM certificate/chain instead of the root CA.                              |

```typescript
const origin = await multi.bindPort(443, {
  protocol: 'https',
  addSsl: {
    upstreamCertValidation: 'disable', // container serves its own self-signed cert
  },
})
```

> [!NOTE]
> For `{ certificate }`, StartOS connects to the container by IP, so the pinned certificate must be valid for that internal IP (present in its SANs). If it isn't, use `'disable'` instead.

Your container chooses the application protocol here, which is the one thing the rewrap adds to [Application protocols](#application-protocols) above. StartOS offers it whichever of `addSsl.alpn` the client also asked for and answers the client with its choice, so both ends of the connection carry one protocol — a container advertising `h2` is reached over `h2` by a client that asked for `h2`, and one that selects nothing leaves the client with no negotiated protocol, which an HTTP client treats as HTTP/1.1. Leave `alpn` unset unless you need to keep this binding off a protocol your container would otherwise select.

Advertise exactly the protocols your container can serve on its own listener. The client is only ever given one your container selects, so a list narrower than what your container speaks costs clients the better protocol — and a client sharing none of them is refused by your container, which reaches the client as a TLS alert naming the hostname rather than the protocol.

Advertising `h2` is the case to think about twice, and worth re-checking on a container that advertises it today: it has to answer extended CONNECT. It commits your container to serving every HTTP/2 client, including WebSockets. On an `https` or `wss` binding StartOS advertises HTTP/2 extended CONNECT ([RFC 8441](https://www.rfc-editor.org/rfc/rfc8441)) to the client whether or not your container implements it, so a browser opens its WebSocket that way and your container has to answer it. A container that advertises only `http/1.1` keeps those clients on HTTP/1.1, where a WebSocket is an ordinary `Upgrade`.

## Serving Your Own TLS (Passthrough)

There is a third arrangement, distinct from both plain termination and the rewrap: **passthrough**, where your container's certificate reaches the client unmodified. Set `secure: { ssl: true }` with **no** `addSsl`:

```typescript
const origin = await multi.bindPort(10009, {
  protocol: null,
  addSsl: null,
  preferredExternalPort: 10009,
  secure: { ssl: true },
})
```

StartOS still fronts the port with one of its TLS listeners, but that listener pipes the raw TLS stream through instead of terminating it, so nothing about the handshake is rewritten. The container sees the client's real source address rather than the proxy's — except for a client on the box itself, which appears as the bridge IP.

### When to use it

Reach for passthrough only when the rewrap genuinely cannot serve, which is one of two cases:

1. **The client must verify your container's own certificate.** A wallet that pins a certificate carried in a connection URI can only do so if the certificate it pins is the one actually served.
2. **The client must see a protocol your container never selects.** A rewrap hands the client whatever your container chose, so a client that requires one — gRPC-go rejects a connection with no selected ALPN (`missing selected ALPN property`) — is served as long as your container's listener advertises it. Reach for passthrough here only when your container's listener cannot be made to advertise it. LND binds its gRPC interface as a passthrough.

Otherwise prefer `addSsl`. Passthrough gives up everything the proxy does on your behalf:

| Capability                       | `addSsl`                      | Passthrough                                 |
| -------------------------------- | ----------------------------- | ------------------------------------------- |
| Certificate the client sees      | The device certificate        | Your container's                            |
| Proxy auth (`addSsl.auth`)       | Available                     | Not available — `auth` lives under `addSsl` |
| `X-Forwarded-*` headers          | Available                     | Not applicable                              |
| ACME on a custom domain          | StartOS obtains and renews it | Skipped — your container is the ACME client |
| UDP on the same port             | Not applicable                | No; the port accepts TLS only               |
| Certificate issuance and renewal | Handled by the platform       | Yours to handle                             |

### Minting the certificate

`sdk.getSslCertificate` returns a PEM fullchain — leaf, intermediate, StartOS root CA — for the hostnames you name, and `sdk.getSslKey` returns the matching key. Because the chain terminates at the StartOS root CA, a client that already trusts the box validates your certificate without pinning anything.

**The SANs are the whole contract.** Nothing rewrites the handshake, so the certificate must be valid for every address a client actually dials — there is no proxy to paper over a mismatch:

- `sdk.getOsIp` (`10.0.3.1`) — the bridge, where other services reach you
- `127.0.0.1` — your own subcontainers, which share the service's network namespace
- `sdk.getContainerIp` — the container itself
- **every address the interface is published at** — LAN IPs, the server's `.local` name, private and public domains, and any onion a plugin has exported

Those three constants are the internal half, and naming only them is the mistake to avoid: an off-box client dialing the LAN address or the `.local` name gets a certificate matching none of them. Take the external half from the interface itself, so it follows the addresses the operator adds and removes:

```typescript
export const setupCerts = sdk.setupOnInit(async effects => {
  const served = await sdk.host
    .getOwn(effects, 'grpc', host => {
      const iface =
        host &&
        Object.values(host.bindings)
          .flatMap(b => Object.values(b.interfaces))
          .find(i => i.id === 'grpc')
      // everything except the WAN IPv4, which getSslCertificate refuses to sign
      return iface ? iface.addressInfo.matchesAny([{ visibility: 'private' }, { exclude: { kind: 'ipv4' } }]).hostnames.map(h => h.hostname) : []
    })
    .const()

  const hostnames = [await sdk.getContainerIp(effects).const(), '127.0.0.1', await sdk.getOsIp(effects), ...served]
  const cert = (await sdk.getSslCertificate(effects, hostnames).const()).join('')
  const key = await sdk.getSslKey(effects, { hostnames })
  await writeFile('/media/startos/volumes/main/tls.cert', cert)
  await writeFile('/media/startos/volumes/main/tls.key', key)
})
```

`getSslCertificate` signs a LAN address and rejects a routable one, so a WAN IPv4 in the list fails the whole call. See [Narrowing the set](main.md#narrowing-the-set) for why that exclusion is a union rather than a two-field `exclude`.

Read the container IP with `.const()` rather than `.once()`: a container that comes back on a new IP must reissue the certificate, or every client dialing the old one fails verification.

Most daemons read their TLS pair once at startup, so reissuing the file is only half the job — something has to restart the service, or it keeps serving the old certificate against the new address set.

> [!WARNING]
> Do **not** add a `<package-id>.startos` DNS name to the SANs. That overlay DNS is deprecated and slated for removal, and it resolves to the container IP rather than the bridge — so it bypasses the platform entirely. Dependents reach you through the bridge; see [Service-to-Service Networking](service-to-service.md).

A passthrough port carries its external port in `net.assignedSslPort`, the same as an `addSsl` port — which of the two fields is populated says whether the port speaks TLS, not who terminates it. Dependents should read neither field directly; `sdk.host.getBridgeAddress` resolves the binding's derived address and is correct under every arrangement on this page.

## Authenticating at the Proxy

For protocols that StartOS fronts with its reverse proxy (`http`, `https`, `ws`, `wss`), you can gate an interface with HTTP authentication by setting `addSsl.auth`. The OS reverse proxy validates the `Authorization` header on every incoming request _before_ forwarding it to your container. Requests that fail get `401 Unauthorized` with a `WWW-Authenticate` challenge and never reach your service. You do **not** need to build auth into the service or run a sidecar proxy — the platform enforces it at the edge.

`auth` takes a `ProxyAuth`, which is one of two shapes:

```typescript
// Basic — one or more username/password pairs; any match passes
const uiOrigin = await uiMulti.bindPort(uiPort, {
  protocol: 'http',
  addSsl: {
    auth: {
      type: 'basic',
      credentials: [{ username: 'admin', password }],
      realm: null, // advertised in the WWW-Authenticate challenge; defaults to "StartOS"
    },
  },
})

// Bearer — any of the listed tokens is accepted as `Authorization: Bearer <token>`
const apiOrigin = await apiMulti.bindPort(apiPort, {
  protocol: 'https',
  addSsl: {
    auth: { type: 'bearer', tokens: [apiToken], realm: null },
  },
})
```

| `ProxyAuth` field     | Type                            | Description                                                                                                                                                          |
| --------------------- | ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `type`                | `'basic'` \| `'bearer'`         | The auth scheme the proxy enforces.                                                                                                                                  |
| `credentials` (basic) | `Array<{ username, password }>` | Accepted pairs. Any match passes. The matched `username` is forwarded upstream as `X-Forwarded-User`.                                                                |
| `tokens` (bearer)     | `Array<string>`                 | Accepted bearer tokens. Any match passes.                                                                                                                            |
| `realm`               | `string` \| `null`              | Realm advertised in the 401 `WWW-Authenticate` challenge. Defaults to `"StartOS"`. Use a stable realm across bindings that share credentials so browsers reuse them. |

Setting `auth` implies HTTP-aware proxying, so it is only valid on the SSL-variant protocols above — not on raw TCP (`protocol: null`).

> [!NOTE]
> The `username` field on `createInterface` is unrelated to this gate — it only embeds a username in the _displayed_ URL (e.g. `https://user@host/`). The enforced credential check is `addSsl.auth`.

### Generating and rotating credentials

Don't hard-code the password. Generate it at install time and let the user rotate it through an action. Store the credential in a [file model](./file-models.md) such as `store.json` and read it reactively in `setupInterfaces` — when the action rewrites the stored value, `setupInterfaces` re-runs and the proxy picks up the new credential automatically:

```typescript
export const setInterfaces = sdk.setupInterfaces(async ({ effects }) => {
  const password = await storeJson.read(s => s.uiPassword).const(effects)

  const uiMulti = sdk.MultiHost.of(effects, 'ui-multi')
  const uiOrigin = await uiMulti.bindPort(uiPort, {
    protocol: 'http',
    addSsl: {
      auth: { type: 'basic', credentials: [{ username: 'admin', password }], realm: null },
    },
  })

  const ui = sdk.createInterface(effects, {
    name: i18n('Web UI'),
    id: 'ui',
    description: i18n('The web interface'),
    type: 'ui',
    masked: false,
    schemeOverride: null,
    username: null,
    path: '',
    query: {},
  })

  return [await uiOrigin.export([ui])]
})
```

Seed `uiPassword` with a generated value during [install init](./init.md) so the gate is active from first start, and pair it with a `reset-password` action that rewrites the stored value and surfaces it to the user once. See [Reset Password](./recipe-reset-password.md).
