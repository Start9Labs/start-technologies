# Trusted Keys

Trusted Keys lists the individual public keys whose signatures your server accepts for packages and StartOS updates. Package trust and StartOS-update trust are independent of each other and of the registry hosting the software.

## Viewing Keys

Open **System → Trusted Keys**. Each row shows a key's label and first identity, its type (GPG or PEM), whether it is trusted for **Packages** and for **StartOS updates**, and its **Linked registries**, the connected registries that offer it. An identity is a GPG user ID, or for a PEM key the email address its registry publishes.

Choose **View details** from a key's menu to see every identity, who added it and when, its linked registries, its fingerprint, and its full public key, which you can copy, with separate **Enabled for service packages** and **Enabled for StartOS updates** toggle rows. The same details open from the **Signed by** list of a package or StartOS update, and from a key notification. GPG keys show their complete OpenPGP fingerprints; PEM keys show a SHA-256 fingerprint of their canonical public-key encoding. A registry's **Manage keys** action in **System → Registries** opens Trusted Keys filtered to the keys that registry offers.

## Changing Trust

Use the **Packages** and **StartOS updates** switches in the table to change a key's trust. The active switch shows a spinner while the request runs. On mobile, these actions move into the key's overflow menu. Each key's menu also offers **Edit label** and, when permitted, **Remove**. At least one key must stay trusted for StartOS updates. Keys offered by a connected registry cannot be removed; distrust them instead.

New installations include the drbonez, matthill, and Start9 signing keys, trusted for packages and StartOS updates. Registry build keys come from connected registries. The 0.4.1.2 migration removes previously seeded Alpha build keys; connected registries can offer them again as untrusted keys. Other existing trust selections are preserved.

## Adding Keys

Click **Add key** and paste an Ed25519 public key into **Public key**, either as a PEM public key or as an armored GPG public-key export. A PEM key also asks for a **Label**; a GPG key is labeled with its GPG identity name. Each signing key in a GPG export gets its own row. A new key is trusted for packages only. Adding a key that is already present keeps its label and trust and merges any new GPG identities.

GPG imports retain the complete original public export, including user IDs, subkeys, expiry, and revocation information. Signing trust uses Ed25519 primary keys and Ed25519 signing subkeys; encryption keys and keys revoked in the pasted export are excluded. Revocation signatures are verified during import. Trust selections govern imported keys independently of GPG certificate lifetimes. Exports without a supported signing key are rejected.

Confirm public keys through a source you already trust. A label or self-certified GPG identity does not establish that a key belongs to the person or organization it names.

## Keys From Registries

A registry chooses which signers to offer and supplies their public keys, GPG identities, and contact details. When you add a registry, the keys it offers that your server does not already have are trusted for packages, and StartOS lists them. Keys your server already has keep their trust. A GPG key a registry adds is labeled with its GPG identity; when another registry offers the same key, its identities are merged and its label is kept.

Whenever StartOS fetches packages or StartOS updates from a connected registry, it checks for keys the registry offers that your server does not have. Each is added untrusted, with a notification that opens the key so you can review and trust it. When a registry stops offering a key, the key keeps its trust and a notification names it; that notification is a warning when the key is trusted for StartOS updates or the registry provides StartOS updates.

Choosing a registry for StartOS updates trusts the keys it offers for StartOS updates. When you remove a registry, StartOS offers to remove the keys it added that no connected registry still offers. See [Marketplace](marketplace.md#managing-registries).

## Installing Software

Package details show up to three signing keys: trusted keys first, otherwise known but untrusted keys, otherwise unknown keys with their fingerprints. **View all** opens the complete list whenever any key is hidden, including when there are more than three signers. Select a key to open its details. An unknown key needs a label before you trust it, and adding it also trusts it for packages. When no trusted signer matches, the Marketplace and the Updates tab show a warning, and installing or updating the package asks you to confirm the risks first. [Sideloading](sideloading.md) works the same way. OS updates from the UI require a signature from a key trusted for StartOS updates; otherwise the update is blocked, and the dialog links to Trusted Keys. The update dialog's **Signed by** lists every key that signed the update and whether you trust it for StartOS updates; select a key to see its details and change its trust.

CLI package installs and OS updates also require trusted signers by default. To deliberately accept an untrusted signer, use `--trust`:

```sh
start-cli package install hello-world --trust
start-cli package install --sideload ./package.s9pk --trust
start-cli server update https://registry.start9.com --trust
```

> [!WARNING]
> Untrusted software can steal private keys and passwords, compromise your server, or destroy data. The override bypasses signer trust, not signature or integrity verification.

Diagnostic updates use PEM keys in `/etc/startos/os-signers`, seeded with the drbonez, matthill, and Start9 keys. Changes to StartOS-update trust save the selected keys in the config overlay, where diagnostic mode can read them without the data drive. The saved selection replaces the defaults. To accept an untrusted signer in diagnostic mode, run `start-cli diagnostic update --trust` locally on the server as root or a member of the `startos` group. The server requires loopback access and its local authentication cookie for that override; remote requests cannot use it.

The existing OS-signer CLI commands continue to manage StartOS-update trust:

```sh
start-cli server update-signers list
start-cli server update-signers add "$(cat signer.pem)" --label "My release signer"
start-cli server update-signers get "$(cat signer.pem)"
start-cli server update-signers update "$(cat signer.pem)" --label "Renamed signer"
start-cli server update-signers remove "$(cat signer.pem)"
```

Keys accept PEM or Der64 (padded standard Base64 DER); `list` returns Der64. Use Trusted Keys to change package trust independently.
