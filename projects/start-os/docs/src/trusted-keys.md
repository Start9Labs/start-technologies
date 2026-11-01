# Manage Trust

Manage Trust lists the individual public keys whose signatures your server accepts for packages and OS updates. Package trust and OS-update trust are independent of each other and of the registry hosting the software.

## Manage Keys

Open **System → Manage Trust**, below **Manage Registries**. Each row shows the key's self-certified GPG name and email addresses, or a label when no GPG identity is available, together with its fingerprint and origin. GPG keys show their complete OpenPGP fingerprints; PEM keys show a SHA-256 fingerprint of their canonical public-key encoding. Open **Edit** from a row's menu to inspect or copy the full public key.

The **Origin** column shows the adding registry's title and logo, **System** for built-in Start9 keys, or **Manually Added**. Registry provenance retains the title and logo recorded at first import, including after removal. The **Packages** and **OS Updates** switches control which signatures the server accepts. Enabling package trust does not enable OS-update trust.

System keys and keys listed by a connected registry cannot be deleted. Disable their trust switches to stop accepting their signatures. Other keys can be deleted from the row's overflow menu. Removing a registry leaves its keys and their permissions unchanged; keys become deletable when no connected registry lists them.

New installations include the drbonez, matthill, and Start9 signing keys, enabled in both contexts. The Alpha and community-registry build keys start disabled in both contexts. Upgrades preserve existing trust selections, including intentionally disabled keys.

To import keys, click **Add key**, enter a **Label**, and paste one or more Ed25519 public keys into **Public keys**. The form accepts PEM public keys and armored GPG public-key exports, including a mixture of both formats. Each signing key gets its own row, with package trust enabled and OS-update trust disabled. Reimporting an existing key preserves its permissions.

GPG imports retain the complete original public export, including user IDs, subkeys, expiry, and revocation information. Signing trust uses Ed25519 primary keys and Ed25519 signing subkeys; encryption keys and keys revoked in the pasted export are excluded. Revocation signatures are verified during import. Trust switches govern imported keys independently of GPG certificate lifetimes. Exports without a supported signing key are rejected.

Confirm public keys through a source you already trust. A label or self-certified GPG identity does not establish that a key belongs to the person or organization it names.

## Registry Trust Requests

Open **System → Manage Registries** to add or remove registries. When adding a registry, StartOS shows the keys it asks you to trust. Review the keys before confirming. Approved keys appear in Manage Trust with **Packages** enabled; this does not enable **OS Updates**.

A registry chooses which of its signers to propose. Use **Review keys** beside an existing registry to review its latest proposal. Confirming enables package trust for the listed keys, including any you previously disabled. Browsing a registry does not grant trust to its keys.

## Installing Software

Package details show a **Signed by** row with each signer's GPG name and email addresses or saved label. Unrecognized signers show their fingerprint. Disabled keys remain identifiable here; the trust switches still determine whether their signatures are accepted. Marketplace installation is blocked when none match. [Sideloading](sideloading.md) allows an untrusted package after an explicit risk confirmation. OS updates from the UI require a signature from an enabled OS-update key.

CLI package installs and OS updates also require trusted signers by default. To deliberately accept an untrusted signer, use `--trust`:

```sh
start-cli package install hello-world --trust
start-cli package install --sideload ./package.s9pk --trust
start-cli server update https://registry.start9.com --trust
```

> [!WARNING]
> Untrusted software can steal private keys and passwords, compromise your server, or destroy data. The override bypasses signer trust, not signature or integrity verification.

Diagnostic mode cannot access the saved trust list. Diagnostic updates require a built-in Start9 key unless explicitly overridden with `start-cli diagnostic update --trust`.

The existing OS-signer CLI commands continue to manage OS-update trust:

```sh
start-cli server update-signers list
start-cli server update-signers add "$(cat signer.pem)" --label "My release signer"
start-cli server update-signers get "$(cat signer.pem)"
start-cli server update-signers update "$(cat signer.pem)" --label "Renamed signer"
start-cli server update-signers remove "$(cat signer.pem)"
```

Keys accept PEM or Der64 (padded standard Base64 DER); `list` returns Der64. Use Manage Trust to change package trust independently.
