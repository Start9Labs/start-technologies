# HTTP Proxy Plugins

An HTTP proxy plugin processes requests and responses in front of another service. StartOS lets the user select and order provider-owned profiles independently for each gateway of an HTTP-proxied binding; providers can challenge clients, inspect bodies, and modify responses.

## Declare and publish profiles

This API requires StartOS 0.4.3 and SDK 4.2.0. Add `plugins: ['http-proxy-v0']` and `osVersion: '0.4.3'` to your [manifest](manifest.md). The SDK exposes `sdk.plugin.httpProxy` for that capability.

Publish the complete selectable profile list from your lifecycle code:

```typescript
await sdk.plugin.httpProxy.setProfiles(effects, [
  {
    id: 'standard',
    name: i18n('Standard protection'),
    protocols: ['http1'],
  },
])
```

Profile IDs are stable, nonempty, and unique within the provider. `name` is a localized display name. `protocols` is a nonempty set of HTTP framing protocols the profile supports on both its ingress and continuation sockets: `http1` and `http2`. Advertise what the upstream actually supports. TLS negotiation uses the intersection of the selected profiles and the binding's ALPN filter; plaintext requests with unsupported framing receive HTTP 505.

`setProfiles` replaces the published list. An empty list withdraws every profile. Keep settings in your package's file models and expose editing through [actions](actions.md); StartOS owns the user's attachment choices and ordering.

## Observe attachments

```typescript
const attachments = await sdk.plugin.httpProxy.getAttachments(effects).const(effects)
```

The reader is a standard SDK `Watchable`, supporting `.once()`, `.const()`, `.watch()`, `.onChange()`, and `.waitFor()`. For a variable set of proxy processes, read it inside a [dynamic daemon builder](recipe-dynamic-daemons.md); a `.const(effects)` change then reconciles the daemon set. Reading it in the outer `setupMain` restarts the service when attachments change.

Each attachment contains:

```typescript
{
  binding: { packageId, hostId, internalPort },
  gateway: { kind: 'network', gatewayId },
  profileId,
  ingress: { transport: 'unix', path: ingressPath },
  next: { transport: 'unix', path: continuationPath },
  protocols: ['http1'],
}
```

`gateway` is either `{ kind: 'network', gatewayId }` or `{ kind: 'url-plugin', packageId }`. The URL-plugin package ID identifies the service container originating traffic, not the protected application or this HTTP proxy provider.

Within this provider, the identity is `(binding.packageId, binding.hostId, binding.internalPort, gateway.kind, gateway ID, profileId)`. The gateway ID is `gatewayId` for network gateways and `packageId` for URL plugins. Reordering and changing **Public only** preserve that identity. The same provider/profile can be selected independently on multiple gateways, with separate sockets. Different profiles from the same provider can appear in one chain; each provider/profile pair occurs once within it. Derive process/configuration keys from that tuple rather than the attachment's position.

## Socket ownership and readiness

StartOS automatically mounts `/media/startos/http-proxy` in the provider's outer container and its subcontainers. Each provider sees its own directory. `in` is writable by container root; `next` is OS-owned and read-only. Run socket-owning processes as container root, or arrange permissions for their user within `in`.

For each attachment:

1. Load the selected profile's configuration.
2. Bind and listen on `ingress.path` when ready to process traffic.
3. Forward accepted requests to `next.path` with their incoming HTTP framing.
4. Stop the listener and remove its ingress socket when the attachment is withdrawn.

Use the supplied paths verbatim. Socket paths are runtime values, not persisted configuration. `next` routes to the next selected stage, or to the protected application's original binding, including its upstream TLS validation. Preserve the incoming request's authority and trusted forwarded headers when forwarding. StartOS applies the binding's authentication and forwarded-header policy before the first stage.

If a provider stops, a selected profile disappears, or a socket cannot be reached, the chain fails closed with HTTP 503. StartOS retains the user's selection. It neither skips a stage nor replays the request against the application. Policy changes disconnect existing connections on the binding, including its other gateways and upgraded WebSocket connections.

## Anubis adapter settings

For an [Anubis Unix-socket listener](https://github.com/TecharoHQ/anubis/blob/main/docs/docs/admin/installation.mdx), configure one instance per attachment with these environment values. A profile selected on both a network gateway and Tor needs two instances, each using its own supplied socket pair:

| Variable                | Value                                  |
| ----------------------- | -------------------------------------- |
| `BIND_NETWORK`          | `unix`                                 |
| `BIND`                  | `attachment.ingress.path`              |
| `TARGET`                | `` `unix://${attachment.next.path}` `` |
| `CUSTOM_REAL_IP_HEADER` | `X-Forwarded-For`                      |
| `XFF_STRIP_PRIVATE`     | `false` when protecting LAN clients    |

StartOS strips client-supplied `X-Forwarded-*`, `Forwarded`, and `X-Real-IP` headers at ingress. With the protected binding's `addXForwardedHeaders` enabled, `X-Forwarded-For` supplies the client address. Make this a requirement for profiles whose policy depends on client IP; a binding with header generation disabled supplies no client-IP header. Anubis's private-IP filtering is provider policy: keep LAN addresses when those clients need challenges too. StartOS's network-gateway **Public only** option is separate: private clients bypass the entire chain before Anubis is involved. URL gateways have no such option; their chain handles traffic delivered by the URL-plugin container.

## Gateway scope and trust

Users configure each binding × gateway policy through the waypoints button (**HTTP proxy chain**) in that gateway table's header. The button appears while an HTTP proxy plugin is installed, including when it is stopped or has not published profiles. A chain covers every managed address and exported interface sharing that binding on that gateway, including paths such as `/admin`. Package rebinding preserves all gateway policies. To disable HTTP proxying, the user must first clear every nonempty chain; saved empty policies do not prevent it.

Network policies default to `publicOnly: false`. With **Public only** enabled, clients classified as private by StartOS bypass both the stages and their ALPN restrictions, even while a provider is unavailable. StartOS uses its connection-origin classification, including on-link globally routable clients, rather than the requested hostname or an RFC1918-only test.

URL-plugin policies select traffic from the actual originating service container and require `publicOnly: false`. They are not inferred from URL spelling or forwarded headers. Traffic from a URL plugin without a configured chain does not fall back to the physical network gateway's policy. Direct service-to-service connections to container ports follow the package's own access policy and do not enter these gateway chains.

> [!WARNING]
> A selected provider can read and modify traffic, including credentials and cookies. Configure authentication and policy for every route the binding serves. Stopping or uninstalling a provider blocks traffic selected for its chains until the user repairs or removes the selection.
