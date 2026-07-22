# Administration

Day-to-day registry administration happens through `start-cli registry` from an admin's workstation. This page walks through the common tasks; for full command syntax see the [start-cli registry reference](/start-os/cli-reference.html#registry).

All commands below assume your `start-cli` is pointed at your registry — either via `--registry <url>` on each invocation or via a named `registry` profile in your workspace `.startos/config.yaml` (used with `-r <name>`). See [Setup](host-registry-setup.md) if you haven't configured that yet.

## Signers

A **signer** is a public key authorized to publish a specific package, OS version, or asset. Admins are themselves signers — when you added the first admin, you registered a signer identity with admin privileges.

Register a new signer (without admin rights):

```sh
start-cli registry admin signer add \
  --name "Alice" \
  --contact "alice@example.com" \
  --key "$(cat alice.pub.pem)"
```

The registry returns the signer's ID. Use that ID with `start-cli registry admin signer edit` to update contact info or keys, or `start-cli registry admin signer list` to see everyone registered.

To grant a signer admin privileges (or revoke them), use `start-cli registry admin add <SIGNER_ID>` / `... admin remove <SIGNER_ID>`.

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
start-cli s9pk publish \
  --url https://my-registry.example.com \
  myservice_1.2.0_x86_64.s9pk
```

`publish` signs the `.s9pk` with the local identity key, uploads it to the registry, and registers it in the index. If your registry already has the same `(package id, version, sighash)` indexed, the upload is a no-op except for any new signatures merging in.

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
  <HEADLINE> \
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
