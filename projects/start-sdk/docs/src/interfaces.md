# Interfaces

`setupInterfaces()` defines the network interfaces your service exposes and how they are made available to the user. This function runs on service install, update, and config save.

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
| `addSsl.alpn`                   | `string` \| `null`                                    | ALPN protocol negotiation (e.g., `'h2'`). Usually `null`.                                                                                                                                                                   |
| `addSsl.preferredExternalPort`  | `number`                                              | External port for SSL connections.                                                                                                                                                                                          |
| `addSsl.addXForwardedHeaders`   | `boolean`                                             | Whether to add `X-Forwarded-*` headers.                                                                                                                                                                                     |
| `addSsl.auth`                   | `ProxyAuth` \| `null`                                 | Optional auth gate enforced by the OS reverse proxy. See [Authenticating at the Proxy](#authenticating-at-the-proxy).                                                                                                       |
| `addSsl.upstreamCertValidation` | `'disable'` \| `{ certificate: string }` \| _omitted_ | How the OS validates your container's TLS cert when it [rewraps SSL](#rewrapping-ssl-to-a-tls-container). Omit to validate against the StartOS root CA (default). See [Rewrapping SSL](#rewrapping-ssl-to-a-tls-container). |
| `secure`                        | `{ ssl: boolean }` \| `null`                          | For non-HTTP protocols, whether the connection is secure.                                                                                                                                                                   |

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

| `createRangeInterface` option | Type               | Description                                                            |
| ----------------------------- | ------------------ | ---------------------------------------------------------------------- |
| `id`                          | `string`           | Unique identifier for the range interface.                             |
| `name`                        | `string`           | Display name shown to the user. Wrap with `i18n()`.                    |
| `description`                 | `string`           | Description shown to the user. Wrap with `i18n()`.                     |
| `scheme`                      | `string` \| `null` | Optional transport prefix (e.g. `'tcp'`). Omit for raw UDP/TCP ranges. |

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
