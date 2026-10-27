# Trusted Keys

Trusted keys identify the publishers whose signatures your server accepts for service packages and OS updates. Trust is independent of the registry hosting the software.

## Manage Trusted Keys

Open **🗝️ Manage trusted keys** from either location:

- **Marketplace**: open the registry selector and use the button next to **Add**.
- **System → General Settings**: open the **Software Update** card's overflow menu.

Each named entry contains one or more public keys. The entries are shared between both locations, but their switches are independent: enabling an entry in the Marketplace does not enable it for OS updates.

New installations include **Start9**, containing the drbonez, matthill, and Start9 signing keys, enabled in both lists. **Start9 Alpha** is included but disabled in both lists. Start9 cannot be deleted; switch it off in either list to stop trusting it there. Other entries can be deleted, which removes them from both lists. Deleted entries are not automatically restored.

To add an entry, click **Add**, enter a **Name**, and paste one or more PEM-encoded Ed25519 public keys into **Public keys**. An entry added from the Marketplace starts enabled there and disabled for OS updates. An entry added from Software Update starts enabled in both lists.

Confirm public keys through a source you already trust. A name is only a label; an entry trusts the actual keys it contains.

## Installing Software

Package details show a **Signed by** row listing enabled Marketplace entries whose keys sign the selected package. Marketplace installation is blocked when none match. [Sideloading](sideloading.md) allows an untrusted package after an explicit risk confirmation. OS updates from the UI require a signature from an enabled OS-update entry.

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

Upgrades preserve the keys previously trusted for OS updates, including an intentionally empty list. When individual Start9 keys were removed, the remaining trusted keys become separate entries and the complete Start9 entry stays disabled for OS updates.

The existing CLI commands manage the OS-enabled keys in the shared list:

```sh
start-cli server update-signers list
start-cli server update-signers add "$(cat signer.pem)" --label "My release signer"
start-cli server update-signers get "$(cat signer.pem)"
start-cli server update-signers update "$(cat signer.pem)" --label "Renamed signer"
start-cli server update-signers remove "$(cat signer.pem)"
```

`add` creates a single-key entry enabled in both lists. `update` renames the entry containing that key, including any other keys in the same entry. `remove` removes the key from its entry, deleting the entry when its last key is removed; it cannot remove keys from the protected Start9 entry. Use the management dialog to change either trust switch.
