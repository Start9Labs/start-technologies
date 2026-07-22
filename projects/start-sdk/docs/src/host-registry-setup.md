# Setup

The packaged `startos-registry` service is the supported way to run a registry. Install it from any registry that carries it (the Start9 registry does), complete two first-run actions, and you're ready to publish.

## 1. Install the service

From StartOS, open the Marketplace, find **StartOS Registry**, and install. The service has no external dependencies. Once installed, start it.

On first install, StartOS surfaces two setup tasks under the service. **Both must be completed before the registry is usable.**

## 2. Configure Registry

Run the **Configure Registry** action to set the registry's display name (max 32 characters) and an optional icon. This is what users will see when they browse your registry from another StartOS device.

The registry's hostnames, listen address, Tor proxy, and data directory are managed by StartOS automatically — you don't configure them by hand. As the service's network addresses change (e.g. you add a clearnet domain to the API interface), the configured hostnames update to match.

## 3. Add the first administrator

Run **Add Administrator** to register the first admin. You'll need a PEM-encoded Ed25519 public key, a label, and contact info (email or Matrix handle). Admins can manage signers, publish packages, register OS versions, and edit categories.

To generate a key on your workstation:

```sh
start-cli init-key
start-cli pubkey
```

`init-key` creates an Ed25519 keypair at `~/.startos/id.key.pem` (or `/run/startos/id.key.pem` if running on a StartOS device). `pubkey` prints the public half — that's what you paste into the **Public Key** field of the Add Administrator action.

Treat the private key like an SSH key: it authenticates every admin and publish action you take against the registry. Back it up.

## 4. Point `start-cli` at the registry

All registry operations go through `start-cli registry` (or `start-cli s9pk publish` for uploading packages). Pass the registry's URL with `--registry`:

```sh
start-cli registry --registry https://my-registry.example.com index
```

If you're working inside a packaging workspace, add the registry to its `.startos/config.yaml` under `registry:` as a named profile, then refer to it with `-r <name>`:

```yaml
registry:
  default: https://my-registry.example.com
```

Now `start-cli registry -r default index` works, and because it's named `default` you can drop the flag entirely (`-r` uses the `default` profile when omitted). Outside a workspace, keep passing `--registry <url>` on each command.

## 5. Smoke-test

Confirm the service is reachable and your admin credentials work:

```sh
start-cli registry index
start-cli registry admin list
```

The first lists registry metadata and packages (empty on a fresh install). The second should show the administrator you added in step 3. If either fails, check that the service is running, the API interface is reachable from your workstation, and your identity key matches the public key you registered.

You're now ready to add signers, publish packages, and register StartOS versions. See [Administration](host-registry-administration.md).
