# Setup

The packaged `startos-registry` service is the supported way to run a registry. Install it from any registry that carries it (the Start9 registry does), complete two first-run actions, and you're ready to publish.

## 1. Install the service

From StartOS, open the Marketplace, find **StartOS Registry**, and install. The service has no external dependencies. Once installed, start it.

On first install, StartOS surfaces two setup tasks under the service. **Both must be completed before the registry is usable.**

## 2. Configure Registry

Run the **Configure Registry** action to set the registry's display name (max 32 characters), an optional icon, and an optional description. This is what users will see when they browse your registry from another StartOS device; the description appears above the registry's services and may use markdown.

Use the description to identify who runs the registry, how to contact them, what packages it carries, and what support users can expect. Include a link to your package-signing public keys and instructions for trusting them.

The registry's hostnames, listen address, Tor proxy, and data directory are managed by StartOS automatically — you don't configure them by hand. As the service's network addresses change (e.g. you add a clearnet domain to the API interface), the configured hostnames update to match.

### Publish signing keys

On StartOS 0.4.1 and 0.4.1.1, Marketplace installs and updates of a package are blocked unless one of its signing keys is in a keyring the user has enabled for service packages. Until users trust a signing key, the Marketplace install button stays disabled for those packages. Users add keys under **System → Trusted keyrings → Add**; see [Trusted Keyrings](/start-os/trusted-keys.html).

Publish your public keys in PEM (`start-cli pubkey` prints yours) or armored GPG format. A registry signer's permission to publish and a user's decision to trust its signatures are separate.

Publish the public keys, and the fingerprint of any GPG key, through an independent source users already trust, such as your website or GitHub profile, and link that source from the description. Users should confirm the key there before importing it; a key supplied only by the registry cannot establish authenticity if the registry is compromised. Publish public keys only, never private keys.

For users on StartOS 0.4.1 or 0.4.1.1, replace these example links with your own:

```markdown
Maintained by Example Community. Contact: support@example.com.

Experimental community packages; support is best-effort through our
[issue tracker](https://github.com/example/packages/issues).

Our [package-signing public key](https://example.com/signing-key.pem)
is also on our [GitHub profile](https://github.com/example). Confirm the
two match, then add it under System → Trusted keyrings → Add. See
[Trusted Keyrings](https://docs.start9.com/start-os/trusted-keys.html).
```

## 3. Add the first administrator

Run **Add Administrator** to register the first admin. You'll need a PEM-encoded Ed25519 public key, a label, and contact info (email or Matrix handle). Admins can manage signers, publish packages, register OS versions, and edit categories.

To generate a key on your workstation:

```sh
start-cli init-key
start-cli pubkey
```

`init-key` creates an Ed25519 keypair at `~/.startos/id.key.pem` (or `/run/startos/id.key.pem` if running on a StartOS device). `pubkey` prints the public half — that's what you paste into the **Public Key** field of the Add Administrator action.

Treat the private key like an SSH key: it authenticates every admin and publish action you take against the registry. Back it up.

To keep the key in gpg-agent instead, skip `init-key` and point `start-cli` at an Ed25519 signing key in your GnuPG keyring (GnuPG 2.3 or later). Set `id-key-gpg: <key>` in `~/.startos/config.yaml` or pass `--id-key-gpg <key>`, where `<key>` is anything `gpg` accepts: a fingerprint, key ID, or user ID. If several signing keys match, append `!` to a fingerprint to choose one. `pubkey` then prints that key. Servers running StartOS 0.4.0.3 or earlier cannot verify an asset signed only by a gpg-agent key.

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
