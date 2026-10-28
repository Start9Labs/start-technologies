# Trusted Keyrings

Trusted keyrings group the public keys whose signatures your server accepts for service packages and OS updates. Trust is independent of the registry hosting the software.

## Manage Trusted Keyrings

Open **Manage trusted keyrings** from either location:

- **Marketplace**: open the overflow menu next to the sort selector.
- **System → General Settings**: open the **Software Update** card's overflow menu.

Each named keyring contains one or more public keys. Keyrings are shared between both locations, but their switches are independent: enabling a keyring in the Marketplace does not enable it for OS updates.

New installations include **Start9**, containing the drbonez, matthill, and Start9 signing keys, enabled in both contexts. **Start9 Alpha** contains the Alpha and community-registry build keys and starts disabled in both contexts. Start9 cannot be deleted; switch it off in either context to stop trusting it there. Other keyrings can be deleted, which removes them from both contexts. Deleted keyrings are not automatically restored.

To add a keyring, click **Add**, enter a **Name**, and paste one or more Ed25519 public keys into **Public keys**. The form accepts PEM public keys and armored GPG public-key exports, including a mixture of both formats. GPG imports store Ed25519 primary keys and Ed25519 signing subkeys as PEM public keys; encryption keys and keys revoked in the pasted export are ignored. Revocation signatures are verified during import. Keyring switches govern imported keys independently of GPG certificate lifetimes. Exports without a supported signing key are rejected.

A keyring added from the Marketplace starts enabled there and disabled for OS updates. A keyring added from Software Update starts enabled in both contexts. Enabling a keyring trusts any of its keys in that context.

Confirm public keys through a source you already trust. A name is only a label; a keyring trusts the actual keys it contains.

## Installing Software

Package details show a **Signed by** row listing enabled Marketplace keyrings whose keys sign the selected package. Marketplace installation is blocked when none match. [Sideloading](sideloading.md) allows an untrusted package after an explicit risk confirmation. OS updates from the UI require a signature from an enabled OS-update keyring.

> [!IMPORTANT]
> Packages signed only by Start9 Alpha or other untrusted keys are blocked under the defaults. Production Marketplace packages must be re-signed with an enabled Start9 key before rollout.

CLI package installs and OS updates also require trusted signers by default. To deliberately accept an untrusted signer, use `--trust`:

```sh
start-cli package install hello-world --trust
start-cli package install --sideload ./package.s9pk --trust
start-cli server update https://registry.start9.com --trust
```

> [!WARNING]
> Untrusted software can steal private keys and passwords, compromise your server, or destroy data. The override bypasses signer trust, not signature or integrity verification.

Diagnostic mode cannot access the saved trust list. Diagnostic updates require a built-in Start9 key unless explicitly overridden with `start-cli diagnostic update --trust`.

## Existing OS-Update Trust

Upgrades preserve the keys previously trusted for OS updates, including an intentionally empty list. When individual Start9 keys were removed, the remaining trusted keys become separate keyrings and the complete Start9 keyring stays disabled for OS updates.

The existing CLI commands manage the OS-enabled keys in the shared keyrings:

```sh
start-cli server update-signers list
start-cli server update-signers add "$(cat signer.pem)" --label "My release signer"
start-cli server update-signers get "$(cat signer.pem)"
start-cli server update-signers update "$(cat signer.pem)" --label "Renamed signer"
start-cli server update-signers remove "$(cat signer.pem)"
```

`add` creates a single-key keyring enabled in both contexts. If the key already belongs to an OS-disabled keyring, it enables that keyring for OS updates without changing its name or Marketplace switch; adding an already OS-trusted key is an error. `update` renames every OS-enabled keyring containing that key, including any other keys in those keyrings. `remove` removes the key from every removable OS-enabled keyring containing it, deleting keyrings whose last key is removed. For protected Start9, `remove` disables the entire keyring for OS updates without changing its keys or Marketplace switch; `add` re-enables it. Use the management dialog to change either trust switch.
