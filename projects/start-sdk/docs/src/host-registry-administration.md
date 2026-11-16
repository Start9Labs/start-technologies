# Administration

Day-to-day registry administration happens through `start-cli registry` from an admin's workstation. This page walks through the common tasks; for full command syntax see the [start-cli registry reference](/start-os/cli-reference.html#registry).

All commands below assume your `start-cli` is pointed at your registry — either via `--registry <url>` on each invocation or via a named `registry` profile in your workspace `.startos/config.yaml` (used with `-r <name>`). See [Setup](host-registry-setup.md) if you haven't configured that yet.

## Signers

A **signer** is a public key authorized to publish a specific package, OS version, or asset. Admins are themselves signers — when you added the first admin, you registered a signer identity with admin privileges.

Publishing authorization does not configure users' package trust. When adding or changing a package-signing key, [publish its public key](host-registry-setup.md#publish-signing-keys) so users can verify it and configure package trust. Put registry ownership and support contacts in the registry description; registering an administrator's contact details is not a substitute.

Register a new signer (without admin rights):

```sh
start-cli registry admin signer add \
  --name "Alice" \
  --contact "mailto:alice@example.com" \
  --key "$(cat alice.pub.pem)"
```

The registry returns the signer's ID. Use that ID with `start-cli registry admin signer edit` to update contact info or keys, or `start-cli registry admin signer list` to see everyone registered.

To grant a signer admin privileges (or revoke them), use `start-cli registry admin add <SIGNER_ID>` / `... admin remove <SIGNER_ID>`.

## Offering signing keys to users

Registry 1.4.0 and start-cli 2.7.0 support selecting registered signers whose keys the registry offers to StartOS 0.4.1.2 and later. New and upgraded registries start with no selection; registering an administrator or authorizing a package publisher does not select them.

As an administrator, list your registered signers and use the [trust-signer selection command](/start-os/cli-reference.html#start-cli-registry-info-set-trust-signers-signer) to select their IDs. Each call replaces the entire selection; passing no IDs clears it. The registry offers all keys belonging to the selected signers, along with their names and contact details. Select the signers whose signatures users should accept, not every identity allowed to upload.

For GPG identities and fingerprints, register the armored public-key export with `admin signer add --key` or reimport it with `admin signer edit --add-key`. A record containing only a DER public key needs the complete GPG export to supply those details.

Remove a retained GPG export with `start-cli registry admin signer edit <SIGNER_ID> --remove-gpg-export <PRIMARY_FINGERPRINT>`. Find primary-key fingerprints under `gpgExports[].fingerprints` in `admin signer list --format json`. Matching is case-insensitive and removes each whole export containing that fingerprint, including exports containing multiple certificates. Repeat the flag to select more fingerprints. The signer's signing keys and publishing permissions remain unchanged; use `--remove-key` to revoke a signing key.

When users connect the registry, StartOS trusts offered keys it does not already have for packages; existing keys keep their trust. Newly discovered keys are imported untrusted when StartOS next fetches packages or OS updates from the registry, with notifications for users to review. Dropping an offered key does not revoke users' trust. Choosing the registry for StartOS updates separately trusts its offered keys for that purpose. See [Keys From Registries](/start-os/trusted-keys.html#keys-from-registries) for the client behavior and trust controls.

## Packages

### Authorizing a signer for a package

Before a non-admin signer can publish a package, an admin (or an already-authorized signer for the same package) must scope them to it:

```sh
start-cli registry package signer add <PACKAGE_ID> <SIGNER_ID> \
  --versions ">=1.0.0"
```

`--versions` is a version range — Alice can publish any version in the range you grant her. Admins can publish any package at any version without an explicit scope.

### Publishing a package

From the directory containing the `.s9pk`:

```sh
start-cli s9pk publish myservice_1.2.0_x86_64.s9pk
```

`publish` uploads the original file to the configured S3 bucket and adds its URL and identity-key endorsement to the configured registry index. Configure the S3 destination and registry before publishing. To index an already hosted file, use `start-cli registry package add <S9PK_FILE> --url <PACKAGE_URL>`.

For package add and add-mirror, the CLI submits verified endorsements from `<S9PK_FILE>` separately from the authenticated uploader's signature. The registry verifies every submitted endorsement against the indexed package's exact Merkle root, signed bound, and `s9pk` context before adding it to the asset signature map. An invalid entry rejects the request atomically. The hosted file may contain only the base archive, and its bytes remain unchanged. Promotion forwards the indexed signature map to the destination registry.

Supplemental keys do not become registered signers, proposed trust keys, or authorized uploaders. Publishing authorization remains tied to the registered uploader.

### Promoting a package

Copy a version's indexed assets to another registry with `start-cli registry package promote <PACKAGE_ID> <VERSION> --from <SOURCE> --to <DESTINATION>`. Both endpoints accept URLs or named `registry:` profiles, using the same configuration as `-r`. For example, with profiles named `alpha` and `production`:

```sh
start-cli registry package promote myservice 1.2.0:0 --from alpha --to production
```

At least one endpoint is required; an omitted endpoint uses the configured registry selected by `-r` or the `default` profile. `start-cli registry os promote <VERSION>` accepts the same endpoint options for OS versions.

### Removing a package

Remove a specific version:

```sh
start-cli registry package remove <PACKAGE_ID> <VERSION>
```

Remove an entire package (all versions):

```sh
start-cli registry package remove <PACKAGE_ID>
```

The second form refuses to run if the package has versions, unless you pass `--force`.

### Mirrors

A mirror is an alternate download URL for the same `.s9pk`. The registry indexes mirrors per-version; downloads try mirrors in order until one succeeds.

```sh
start-cli registry package add-mirror <S9PK_FILE> <MIRROR_URL>
start-cli registry package remove-mirror <PACKAGE_ID> <VERSION> --url <MIRROR_URL>
```

You can't remove the last remaining URL for a package — every indexed version needs at least one reachable URL.

### Cosigning

Any signer known to the registry can add their signature to a version already in the index:

```sh
start-cli registry package sign <S9PK_FILE>
start-cli registry package sign --no-verify --id <PACKAGE_ID> --version <VERSION>
```

The first form verifies a local copy of the `.s9pk` and adds its endorsement to the registry index. To embed an endorsement in a file for sideloading, use `start-cli s9pk sign <S9PK_FILE>`. With `--no-verify`, it signs the commitments the registry lists for that version without checking them against a local file.

## Categories

Categories are flat tags that group packages in the marketplace UI. Create and assign:

```sh
start-cli registry package category add bitcoin "Bitcoin"
start-cli registry package category add-package bitcoin <PACKAGE_ID>
```

A package can be in multiple categories. `start-cli registry package category list` enumerates them.

## StartOS versions

If your registry distributes StartOS images (not just service packages), register each release so devices can find upgrade paths:

```sh
start-cli registry os version add \
  <VERSION> \
  <RELEASE_NOTES> \
  <SOURCE_VERSION_RANGE>
```

`<SOURCE_VERSION_RANGE>` is a version range describing which prior OS versions can upgrade to this one. After registering the version, upload the install images:

```sh
start-cli registry os asset add <FILE> <URL> \
  --platform x86_64 --version <VERSION>
```

Repeat per platform (`x86_64`, `aarch64`, `riscv64`) and per asset type (`img`, `iso`, `squashfs`).

## Inspecting the registry

```sh
start-cli registry index            # registry metadata + every package
start-cli registry package index    # packages and categories only
start-cli registry os index         # OS versions
start-cli registry admin list       # admins
start-cli registry admin signer list  # all signers
```

All listing commands accept `--format json` for machine-readable output.

## Low-level database access

For debugging or scripted recovery, you can read and patch the registry's patch-db directly:

```sh
start-cli registry db dump -p /index/package/packages
start-cli registry db apply '<jq-style expression>'
```

These are powerful and easy to misuse — there's no schema validation on `apply`. Prefer the higher-level commands above unless you're recovering from a bug.
