# Setup

The packaged `startos-registry` service is the supported way to run a registry. Install it from any registry that carries it (the Start9 registry does), name the registry and register an administrator, and you're ready to publish.

## 1. Install the service

From StartOS, open the Marketplace, find **StartOS Registry**, and install. The service has no external dependencies. Once installed, start it.

A fresh install starts one registry, and StartOS surfaces two setup tasks under the service: **Manage Registries** and **Manage Signers**. They don't block the service from starting, but nobody can publish to the registry until Manage Signers has registered an administrator.

## 2. Name and describe the registry

Run the **Manage Registries** action to name the registry (1–32 characters). This is what users see when they browse your registry from another StartOS device. The same action can add more registries to the service, each with its own catalog, signers, and addresses.

Then run **Configure Registry** to set an optional icon and description for it. The description appears above the registry's services and may use markdown.

Use the description to identify who runs the registry, how to contact them, what packages it carries, and what support users can expect. Include a link to your package-signing public keys and instructions for verifying them.

The registry's hostnames, listen address, Tor proxy, and data directory are managed by StartOS automatically — you don't configure them by hand. As the service's network addresses change (e.g. you add a clearnet domain to the API interface), the configured hostnames update to match.

### Publish signing keys

Users manage individual signing keys under **System → [Trusted Keys](/start-os/trusted-keys.html)**. When no key trusted for packages signs a package, Marketplace installs and updates show a warning and ask users to confirm the risks before proceeding.

Before users connect, [select which signing keys your registry offers](host-registry-administration.md#offering-signing-keys-to-users). Connecting the registry trusts offered keys the user's server does not already have for packages; existing keys keep their trust. Users can also import independently published keys under **System → Trusted Keys → Add key**. A registry signer's permission to publish and a user's decision to trust its signatures are separate.

Publish your public keys in PEM (`start-cli pubkey` prints yours) or armored GPG format through an independent source users already trust, such as your website or GitHub profile, and link that source from the description. Include the fingerprint of any GPG key. Users should confirm the key there before connecting the registry or importing the key; a key supplied only by the registry cannot establish authenticity if the registry is compromised. StartOS shows the description only after a user adds the registry, when newly imported offered keys are already trusted for packages. Share the independent key source wherever you share the registry URL so users can verify before connecting. Publish public keys only, never private keys.

For example, replace these links with your own:

```markdown
Maintained by Example Community. Contact: support@example.com.

Experimental community packages; support is best-effort through our
[issue tracker](https://github.com/example/packages/issues).

Our [package-signing public key](https://example.com/signing-key.pem)
is also on our [GitHub profile](https://github.com/example). Confirm the
two match and that System → Registries → Manage keys shows the same key
for this registry. If anything differs, remove this registry and the keys
it added. Manage package trust under System → Trusted Keys; see
[Trusted Keys](https://docs.start9.com/start-os/trusted-keys.html).
```

## 3. Add the first administrator

Run **Manage Signers** and add an entry to the registry's signer list. Give it a label and paste your public key into **Public Keys**: a PEM-encoded Ed25519 public key, or a complete armored GPG export of an Ed25519 key. Contacts (email addresses, Matrix usernames, or URLs) are optional. Turn on **Admin**. Admins can manage signers, publish packages, register OS versions, and edit categories.

**Offer for Trust**, on the same entry, is separate from **Admin**: it offers the signer's keys to users who connect the registry. See [Offering signing keys to users](host-registry-administration.md#offering-signing-keys-to-users) before turning it on.

To generate a key on your workstation:

```sh
start-cli init-key
start-cli pubkey
```

`init-key` creates an Ed25519 keypair at `~/.startos/id.key.pem` (or `/run/startos/id.key.pem` if running on a StartOS device). `pubkey` prints the public half — that's what you paste into the **Public Keys** field in Manage Signers.

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

Because it's named `default`, `start-cli registry index` now uses it with no flag. Give any other registry its own name and select it with `-r <name>`. Don't pass `-r default`: the flag itself sets the `default` profile, so naming `default` refers back to itself and fails with a profile reference cycle. Outside a workspace, keep passing `--registry <url>` on each command.

## 5. Smoke-test

Confirm the service is reachable and your admin credentials work:

```sh
start-cli registry index
start-cli registry admin list
```

The first lists registry metadata and packages (empty on a fresh install). The second should show the administrator you added in step 3. If either fails, check that the service is running, the API interface is reachable from your workstation, and your identity key matches the public key you registered.

You're now ready to add signers, publish packages, and register StartOS versions. Before inviting users to connect, [select which signing keys your registry offers](host-registry-administration.md#offering-signing-keys-to-users). See [Administration](host-registry-administration.md) for these operations.
