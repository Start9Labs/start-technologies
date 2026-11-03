# S9PK Package Format

S9PK is the package format for StartOS services. Version 2 uses a merkle archive structure for efficient downloading and cryptographic verification.

## File Format

S9PK files begin with a 3-byte header: `0x3b 0x3b 0x02` (magic bytes + version 2).

The embedded Ed25519ph signature uses context `s9pk` and signs the 32-byte BLAKE3 Merkle root followed by its exact unsigned 64-bit big-endian root TOC bound. Readers always verify the embedded signature, including when no external commitment is supplied.

## Supplemental signatures (v2)

An optional trailer leaves every base archive byte unchanged:

```
[base archive][TLV region][region length: u64 BE][ASCII S9PKEXT1]
```

The TLV region is at most 1 MiB, excluding the 16-byte footer. Each record is a type (u16 BE), value length (u64 BE), and that many bytes. Type 1 contains UTF-8 JSON with string fields `signer` (`AnyVerifyingKey`) and `signature` (`AnySignature`): padded Base64 DER, accepting existing PEM and Base64url aliases. Unknown types and JSON fields are ignored. Trailer-only editing preserves original records and unknown fields byte-for-byte.

Signatures bind the archive-derived root and exact signed bound, with context `s9pk`. Both canonical Ed25519ph and pure Ed25519 commitments are supported, including legacy ph signatures labeled as pure Ed25519. Verification uses strict, uncofactored Ed25519 checks. Pure signing uses the existing `Start-Commitment-Sig v1\0` protocol tag, length-prefixed context, commitment type 2, and the same 40 commitment bytes. Keys in metadata express endorsements, not trust or publishing permission. Canonical identity deduplication retains the embedded entry on collision.

The footer family reserves `S9PKEXT` followed by one version byte. An unsupported version is diagnosed and ignored. Structural TLV failure, oversized regions, overflow, or overlap with any referenced TOC or lazy file discards all trailer endorsements. With valid framing, malformed JSON (including duplicate decoded keys at any nesting level), invalid shapes, and unsupported or invalid signatures are diagnosed per record; valid siblings remain usable. JSON nesting is limited to 64 levels. Optional metadata damage never bypasses or replaces base archive validation.

Discovery seeks directly to the footer. Known-size sources validate every referenced endpoint against file bounds; unknown-length sources report that supplemental signatures cannot be discovered. Optional metadata read failures produce diagnostics and leave verified base metadata usable. Archive TOC and file reads retain the pooled streaming path for GitHub CDN compatibility. Footer and TLV-region discovery alone use bounded HTTP range requests when advertised; ignored or unsupported ranges fall back to streaming and may consume a whole response.

`start-cli s9pk sign <file>` adds an identity-key endorsement through a synchronized temporary-file replacement. Writers validate all physical archive endpoints, prepare the complete capped region before writing, and refuse malformed known records, invalid framing, and unsupported trailer versions. Well-framed unknown or cryptographically unsupported records remain intact. Content-changing archive rewrites discard the old trailer with a diagnostic; endorse the resulting package again.

## Archive Structure

```
/
├── manifest.json           # Package metadata (required)
├── icon.<ext>              # Package icon - any image/* format (required)
├── LICENSE.md              # License text (required)
├── instructions.md         # User-facing setup/usage instructions (required)
├── dependencies/           # Dependency metadata (optional)
│   └── <package-id>/
│       ├── metadata.json   # DependencyMetadata
│       └── icon.<ext>      # Dependency icon
├── javascript.squashfs     # Package JavaScript code (required)
├── assets.squashfs         # Static assets (optional, legacy: assets/ directory)
└── images/                 # Container images by architecture
    └── <arch>/             # e.g., x86_64, aarch64, riscv64
        ├── <image-id>.squashfs  # Container filesystem
        ├── <image-id>.json      # Image metadata
        └── <image-id>.env       # Legacy environment projection
```

## Components

### manifest.json

The package manifest contains all metadata:

| Field                    | Type          | Description                                                 |
| ------------------------ | ------------- | ----------------------------------------------------------- |
| `id`                     | string        | Package identifier (e.g., `bitcoind`)                       |
| `title`                  | string        | Display name                                                |
| `version`                | string        | Extended version string                                     |
| `satisfies`              | string[]      | Concrete versions this version is backwards compatible with |
| `releaseNotes`           | string/object | Release notes (localized)                                   |
| `canMigrateTo`           | string        | Version range for forward migration                         |
| `canMigrateFrom`         | string        | Version range for backward migration                        |
| `license`                | string        | License type                                                |
| `packageRepo`            | string        | StartOS package repository URL                              |
| `upstreamRepo`           | string        | Upstream project URL                                        |
| `marketingUrl`           | string?       | Marketing site URL                                          |
| `donationUrl`            | string?       | Optional donation URL                                       |
| `description`            | object        | Short and long descriptions (localized)                     |
| `images`                 | object        | Image configurations by image ID                            |
| `volumes`                | string[]      | Volume IDs for persistent data                              |
| `dependencies`           | object        | Package dependencies                                        |
| `hardwareRequirements`   | object        | Hardware requirements (arch, RAM, devices)                  |
| `hardwareAcceleration`   | boolean       | Whether package uses hardware acceleration                  |
| `userspaceFilesystems`   | boolean       | Whether package is granted `/dev/fuse`                      |
| `virtualNetworking`      | boolean       | Whether package is granted `/dev/net/tun`                   |
| `hardwareVirtualization` | boolean       | Whether package is granted `/dev/kvm`                       |
| `plugins`                | string[]      | Plugin IDs the package registers                            |
| `gitHash`                | string?       | Git commit hash                                             |
| `osVersion`              | string        | Minimum StartOS version                                     |
| `sdkVersion`             | string?       | SDK version used to build                                   |

### javascript.squashfs

Contains the package JavaScript that implements the `ABI` interface from `@start9labs/start-core` (re-exported via `@start9labs/start-sdk`). This code runs in the container runtime and manages the package lifecycle.

The squashfs is mounted at `/usr/lib/startos/package/` and the runtime loads `index.js`. Packing and v1 conversion set JavaScript and asset inode timestamps to zero without modifying source files.

### images/

Container images organized by architecture:

- **`<image-id>.squashfs`** - Container root filesystem
- **`<image-id>.json`** - Image metadata (entrypoint, user, workdir, and `env`). The `env` object maps variable names to strings or `null` for removal and preserves multiline values.
- **`<image-id>.env`** - Legacy newline-delimited environment projection for older SDKs. The SDK uses it when metadata has no `env` object.

The SDK merges image defaults with per-command overrides and serializes one JSON object into contiguous environment variables `JSON_ENV_0`, `JSON_ENV_1`, and so on. Each UTF-8 chunk is at most 64 KiB. `start-container` concatenates them in numeric order before parsing; missing indexes and malformed JSON fail without echoing values. Subcontainer commands apply sources in order: default `PATH`, available server locales, `--env-file`, the JSON overrides, and explicit `--env` arguments. Transport variables are not inherited by the image command unless explicitly included in those sources. Linux's total exec size and individual service-variable limits still apply.

Packing selects the requested Linux platform from exported image data; configuration and runtime metadata do not require daemon inspection. Registry tags first export from the local Docker/Podman store, then pull and retry when export fails or lacks the requested platform. Docker builds export OCI directly; Podman builds export OCI from its native image store. Local OCI layout directories and tar archives need no daemon. Legacy conversion reads saved images directly without loading them into a container engine.

OCI inputs traverse nested image indexes and select one configuration by platform and optional top-level reference annotation or digest. Referenced and embedded blobs are verified against descriptor sizes and SHA-256/SHA-384/SHA-512 digests and spooled into owned temporary sources. Selected content must be available locally; descriptor URLs do not trigger downloads. Docker-save archives remain supported for daemon export and legacy conversion.

The native tar decoder spools payloads without extracting paths onto the workstation. Reconstruction applies ordered layers, lower-layer whiteouts and opaque directories, and preserves hardlink inode identity across replacements and deletions. Runtime metadata and environment come from the selected archive configuration; each complete uncompressed layer is verified against its configured SHA-256 DiffID. Uncompressed, gzip, and Zstd layers are supported.

The image filesystem retains image-provided inode attributes within the native SquashFS import limits: whole-second timestamps in the unsigned 32-bit range, and `user.`, `trusted.`, and `security.` xattrs. Docker-created container initialization files are not added or removed by filename. Implicit directories use root ownership, mode 0755, and zero timestamps. Symlink ancestors are rejected rather than followed. Image configuration identity, writer options, and the locked Zstd codec determine SquashFS bytes; JavaScript and assets additionally require identical contents and non-timestamp attributes.

The `ImageConfig` in manifest specifies:

- `arch` - Supported architectures
- `emulateMissing` - Whether an available image may run under CPU emulation
- `nvidiaContainer` - Whether to enable NVIDIA container support

### assets.squashfs

Static assets accessible to the package, mounted read-only at `/media/startos/assets/` in the container.

### dependencies/

Metadata for dependencies displayed in the UI:

- `metadata.json` - Just title for now
- `icon.<ext>` - Icon for the dependency

## Merkle Archive

The S9PK uses a merkle tree structure where each file and directory has a blake3 hash. This enables:

1. **Partial downloads** - Download and verify individual files
2. **Integrity verification** - Verify any subset of the archive
3. **Efficient updates** - Only download changed portions
4. **DOS protection** - Size limits enforced before downloading content

Files are sorted by priority for streaming (manifest first, then icon, license, instructions, dependencies, javascript, assets, images).

## Building S9PK

Use `start-cli s9pk pack` to build packages:

```bash
start-cli s9pk pack <package-directory> -o <output.s9pk>
```

Images can be sourced from:

- Docker/Podman builds
- Existing Docker/Podman tags
- Pre-built squashfs files

## Related Code

- `src/s9pk/v2/mod.rs` - S9pk struct and serialization
- `src/s9pk/manifest.rs` - Manifest types
- `src/s9pk/v2/pack.rs` - Packing logic
- `src/s9pk/v2/image.rs` - Image archive selection, decompression, and integrity
- `src/util/squashfs/source/layers.rs` - Image filesystem changesets
- `src/s9pk/merkle_archive/` - Merkle archive implementation
