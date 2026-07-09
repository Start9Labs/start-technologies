# Expose a Web UI

Every service with a browser interface needs at least one HTTP interface. This is the most basic networking pattern — bind a port, create an interface descriptor, and export it.

## Solution

In `setupInterfaces()`, create a `MultiHost` with `sdk.MultiHost.of(effects, 'ui')`, bind an HTTP port with `multi.bindPort(port, { protocol: 'http', preferredExternalPort: 80 })`, create a `'ui'` type interface with `sdk.createInterface()` setting `masked: false`, and export it. Return the receipt array.

**Reference:** [Interfaces](interfaces.md)

## Authentication

**StartOS does not authenticate a bound port.** Binding an interface exposes it; nothing gates it unless you say so. There are exactly two ways to gate it, and they are not interchangeable.

**Prefer the app's own login.** When upstream ships one, feed it the credential it expects — an env var (`SALTED_PASS`, `QBT_PW_HASH`) or its config file — and **persist only the hash**. Follow [Prompt User to Create Admin Credentials](recipe-admin-credentials.md): the action is the sole writer of the credential, it returns the value rather than storing it in cleartext, and the user rotates it by re-running the action. Reference implementations: [qbittorrent](https://github.com/Start9Labs/qbittorrent-startos) (`QBT_PW_HASH`) and [changedetection](https://github.com/Start9-Community/changedetection-startos) (`SALTED_PASS`).

**Fall back to the OS gate only when the app has no auth of its own.** [`addSsl.auth`](interfaces.md#authenticating-at-the-proxy) makes the OS reverse proxy challenge every request to that binding's port, so REST endpoints, RSS feeds, and webhooks are gated along with the browser UI, and any client that can't send an `Authorization` header is locked out. Reference implementation: [searxng](https://github.com/Start9Labs/searxng-startos).

> [!WARNING]
> **Do not derive an app-native login from `searxng-startos`.** It is the OS-gate reference _only_. It stores the password in cleartext because the reverse proxy needs the cleartext to configure basic auth, and it takes the credential `withInput` because the user is choosing public-vs-private. Neither property transfers to an app-native login — copying them puts a plaintext password at rest. Searching for `addSsl.auth` lands you in searxng first; that answers "how does StartOS gate a port," not "how do I wire up the app's own login."

## Examples

See `startos/interfaces.ts` in: [hello-world](https://github.com/Start9Labs/hello-world-startos), [actual-budget](https://github.com/Start9Labs/actual-budget-startos), [filebrowser](https://github.com/Start9Labs/filebrowser-startos), [uptime-kuma](https://github.com/Start9Labs/uptime-kuma-startos), [spliit](https://github.com/Start9Labs/spliit-startos)
