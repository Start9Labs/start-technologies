# Let Dependents Through a Proxy Auth Gate

A binding gated with `addSsl.auth` challenges every client on every address, the plaintext bridge address included. A service that depends on it gets `401` unless it sends a credential the gate accepts. Both sides of the dependency take part: the gated service publishes a machine key, and its dependents read it and send it.

## Solution

**On the gated service:**

1. Generate an `apiKey` in `setupOnInit` whenever it is missing (`utils.getDefaultString`), and write it to `credentials.json` on a `public` volume through a `FileHelper.json` model. Declare `public` in the manifest's `volumes` and include it in `sdk.Backups.ofVolumes(...)`.
2. Admit the key at the gate in the scheme the dependents' client sends. If the binding already takes that scheme, add the key to its `tokens` or `credentials` list. If it doesn't — browsers need Basic and an OpenAI-compatible client only sends Bearer — bind a second `MultiHost` on the same internal port with `auth: { type: 'bearer', tokens: [apiKey] }` and export a `type: 'api'` interface on it. One binding takes exactly one `ProxyAuth`.
3. Export the key-gated host id and the internal port from `startos/utils.ts`.
4. Optionally, add an action that shows the key and rotates it. `setupInterfaces` reads the key with `.const()`, so a rotation rebinds the gate without restarting the daemon.

**On each dependent:**

1. Resolve the key-gated host's bridge address with `sdk.host.getBridgeAddress(...).const()`, importing the host id and port from the dependency's package.
2. Read the key: mount the dependency's `public` volume read-only — `.mountDependency()` when only the daemon reads the file, `sdk.mount()` onto a volume you don't back up when your package code needs the value — and read `apiKey` with `.const()`.
3. Send it as `Authorization: Bearer <key>`, or in whatever form the app's client takes an API key.
4. Raise the dependency's `versionRange` to its first release that publishes the key.

**Reference:** [Authenticating at the Proxy](interfaces.md#authenticating-at-the-proxy) · [Admitting dependent services](interfaces.md#admitting-dependent-services) · [Reaching a gated dependency](service-to-service.md#reaching-a-gated-dependency) · [Mount Volumes from Another Service](recipe-mount-dependency.md)

## Examples

The gated service: [llama-cpp](https://github.com/Start9Labs/llama-cpp-startos) — `startos/interfaces.ts` (Basic-gated Web UI host, Bearer-gated API host on the same port), `startos/init/initializeService.ts`, `startos/fileModels/credentials.json.ts`, `startos/actions/apiKey.ts`. [vllm](https://github.com/Start9Labs/vllm-startos) publishes its key the same way.

A dependent: [DeepSeek Harness](https://github.com/Start9-Community/DeepseekHarnessStart9) — `startos/backends.ts` (`resolveBaseUrl`, `readPublicApiKey`).
