# S9PK Package Format

S9PK is the package format for StartOS services. Version 2 uses a merkle archive structure for efficient downloading and cryptographic verification.

## File Format

S9PK files begin with a 3-byte header: `0x3b 0x3b 0x02` (magic bytes + version 2).

The archive is cryptographically signed using Ed25519 with prehashed content (SHA-512 over blake3 merkle root hash).

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
