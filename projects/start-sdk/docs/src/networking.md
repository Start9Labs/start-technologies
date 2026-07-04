# Networking

StartOS provides a secure, host-mediated networking environment. Services do not connect to each other directly by IP or overlay network. Instead, all inter-service communication routes through the host's internal bridge network.

## The Host Bridge Model

Services on StartOS are strictly isolated. Direct container-to-container communication is disabled by the firewall. To communicate with another service, your service must use the **Host Bridge Address** over plain HTTP.

When a service connects to the host bridge, the StartOS `net_controller` intercepts the traffic and routes it to the target container, providing an enforceable boundary.

### Connecting to a Dependency

To connect to a dependency, you construct a URL using the host's bridge IP address and the dependency's LAN-domain port.

Because the bridge network is intrinsically secure (traffic never leaves the host), service-to-service communication happens over **plain HTTP**, bypassing any TLS wrapping the service might provide for external clients.

### Using the SDK

The StartOS SDK provides a helper to automatically construct the correct service-to-service base URL for a dependency's interface.

```typescript
import { sdk } from './sdk'

// Within an action, init, or main lifecycle hook:
const url = await sdk.getServiceBridgeUrl(effects, 8080)
// url will be e.g. "http://10.0.3.1:8080"
```

> **Note**: Do not hardcode `10.0.3.1` in your scripts. Always obtain the bridge IP from the SDK at runtime, as the bridge subnet is subject to change in future network topologies.

## Deprecation of `.startos`

In older versions of StartOS (0.3.x), services used `.startos` DNS overlay names (e.g., `http://bitcoind.startos:8332`) to communicate. This model suffered from DNS caching issues and is being removed. 

> **Important**: Never use `.startos` domains for new packages. If you are migrating an older package, replace all `.startos` base URLs with the bridge URL obtained from the SDK.
