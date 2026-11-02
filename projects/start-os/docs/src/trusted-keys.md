# Trusted Keys

Trusted Keys lists the individual public keys whose signatures your server accepts for packages and StartOS updates. Package trust and StartOS-update trust are independent of each other and of the registry hosting the software.

## Viewing Keys

Open **System → Trusted Keys**. Each row shows a key's label and first identity, its type (GPG or Ed25519), whether it is trusted for **Packages** and for **StartOS updates**, and the connected registries that offer it. An identity is a GPG user ID, or for a plain Ed25519 key the email address its registry publishes.

Click a row for the key's details: every identity, who added it and when, the registries offering it, its fingerprint, and its full public key, which you can copy. GPG keys show their complete OpenPGP fingerprints; PEM keys show a SHA-256 fingerprint of their canonical public-key encoding. A registry's **Manage keys** action in **System → Registries** opens Trusted Keys filtered to the keys that registry offers.

## Changing Trust

A key's menu and its details offer **Edit label**, trust or distrust for packages, trust or distrust for StartOS updates, and **Remove**. Trusting a key for StartOS updates asks for confirmation. At least one key must stay trusted for StartOS updates. Keys offered by a connected registry cannot be removed; distrust them instead.

New installations include the drbonez, matthill, and Start9 signing keys, trusted for packages and StartOS updates. The Alpha and community-registry build keys start untrusted. Upgrades preserve existing trust selections, including intentionally untrusted keys.

## Adding Keys

Click **Add key**, enter a **Label**, and paste one or more Ed25519 public keys into **Public keys**. The form accepts PEM public keys and armored GPG public-key exports, including a mixture of both formats. Each signing key gets its own row, trusted for packages only. Adding a key that is already present keeps its label and trust and merges any new GPG identities.

GPG imports retain the complete original public export, including user IDs, subkeys, expiry, and revocation information. Signing trust uses Ed25519 primary keys and Ed25519 signing subkeys; encryption keys and keys revoked in the pasted export are excluded. Revocation signatures are verified during import. Trust selections govern imported keys independently of GPG certificate lifetimes. Exports without a supported signing key are rejected.

Confirm public keys through a source you already trust. A label or self-certified GPG identity does not establish that a key belongs to the person or organization it names.

## Keys From Registries

A registry chooses which signers to offer and supplies their public keys, GPG identities, and contact details. When you add a registry, the keys it offers that your server does not already have are trusted for packages, and StartOS lists them. Keys your server already has keep their trust. A GPG key a registry adds is labeled with its GPG identity; when another registry offers the same key, its identities are merged and its label is kept.

Whenever StartOS fetches packages or StartOS updates from a connected registry, it checks for keys the registry offers that your server does not have. They are added untrusted, and a notification offers to review them and trust them for packages.

Choosing a registry for StartOS updates trusts the keys it offers for StartOS updates. When you remove a registry, StartOS offers to remove the keys it added that no connected registry still offers. See [Marketplace](marketplace.md#managing-registries).

## Installing Software

Package details show a **Signed by** row with the labels and email addresses of package signers you trust. When none match, it shows known untrusted signers instead. Unknown signers are omitted, and the row is hidden when no known signer matches. When no trusted signer matches, the Marketplace and the Updates tab show a warning that links to Trusted Keys, and installing or updating the package asks you to confirm the risks first. [Sideloading](sideloading.md) works the same way. OS updates from the UI require a signature from a key trusted for StartOS updates; otherwise the update is blocked, and the dialog links to Trusted Keys.

CLI package installs and OS updates also require trusted signers by default. To deliberately accept an untrusted signer, use `--trust`:

```sh
start-cli package install hello-world --trust
start-cli package install --sideload ./package.s9pk --trust
start-cli server update https://registry.start9.com --trust
```

> [!WARNING]
> Untrusted software can steal private keys and passwords, compromise your server, or destroy data. The override bypasses signer trust, not signature or integrity verification.

Diagnostic mode cannot access the saved trust list. Diagnostic updates require a built-in Start9 key unless explicitly overridden with `start-cli diagnostic update --trust`.

The existing OS-signer CLI commands continue to manage StartOS-update trust:

```sh
start-cli server update-signers list
start-cli server update-signers add "$(cat signer.pem)" --label "My release signer"
start-cli server update-signers get "$(cat signer.pem)"
start-cli server update-signers update "$(cat signer.pem)" --label "Renamed signer"
start-cli server update-signers remove "$(cat signer.pem)"
```

Keys accept PEM or Der64 (padded standard Base64 DER); `list` returns Der64. Use Trusted Keys to change package trust independently.
