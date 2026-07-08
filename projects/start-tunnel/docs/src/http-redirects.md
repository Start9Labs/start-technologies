# HTTP Redirects

StartTunnel runs an **HTTP→HTTPS redirect** on port `80` of every public IPv4 address your VPS holds. A plain `http://` request to one of these addresses is answered with a redirect to the same host over `https://`, so visitors who leave off the scheme still land on your TLS service instead of getting a connection error.

These redirects are **on by default** — on a fresh install and after an update, every public IPv4 gets one automatically. You can turn any of them off, and your choice persists.

## Where they run

A redirect binds a lightweight listener directly on `<public-IPv4>:80` on the tunnel host. Only **public** IPv4 addresses are eligible (loopback and private/RFC1918 ranges are skipped) — the same set the web UI lists. IPv6 is handled separately: the `80 → 443` upgrade for a device's IPv6 address is a [pinhole](./published-ports.md) port translation, not one of these listeners.

## Mutually exclusive with a published port 80

A redirect and a [published port](./published-ports.md) cannot both own port 80 on the same IP — they are never both enabled. The two directions are enforced explicitly rather than by one silently overriding the other:

- **Publishing port 80 while the redirect is on is rejected.** To publish port 80 to a device, first turn the redirect off for that address.
- **Enabling the redirect while port 80 is published is rejected.** Delete the published port 80 first.

Port 80 is also never opened automatically: StartTunnel refuses PCP/UPnP requests to auto-publish it, since the redirect is the intended behavior there.

## Managing them in the UI

The **HTTP Redirect (80 → 443)** section on the `Settings` page lists every public IPv4 with a toggle:

- **On** — the port-80 HTTP→HTTPS redirect is running on that address.
- **Off** — you have turned it off; port 80 there simply refuses plain HTTP (and is free to publish).
- **Disabled toggle** — a published port currently occupies port 80 on that address. Remove the published port to re-enable the redirect.

(Most VPSes have a single public IPv4, so this is usually just one toggle.)

## Managing them from the CLI

```bash
# Show the redirect status of every public IPv4
start-tunnel http-redirect list

# Turn the redirect off for one address (omit --enabled)
start-tunnel http-redirect set-enabled 203.0.113.10

# Turn it back on
start-tunnel http-redirect set-enabled 203.0.113.10 --enabled
```

See the [CLI reference](./cli-reference.md#http-redirects) for details.
