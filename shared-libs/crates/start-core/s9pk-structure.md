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
        └── <image-id>.env       # Environment variables
```

## Components

### manifest.json

The package manifest contains all metadata:

| Field                  | Type          | Description                                |
| ---------------------- | ------------- | ------------------------------------------ |
| `id`                   | string        | Package identifier (e.g., `bitcoind`)      |
| `title`                | string        | Display name                               |
| `version`              | string        | Extended version string                    |
| `satisfies`            | string[]      | Version ranges this version satisfies      |
| `releaseNotes`         | string/object | Release notes (localized)                  |
| `canMigrateTo`         | string        | Version range for forward migration        |
| `canMigrateFrom`       | string        | Version range for backward migration       |
| `license`              | string        | License type                               |
| `wrapperRepo`          | string        | StartOS wrapper repository URL             |
| `upstreamRepo`         | string        | Upstream project URL                       |
| `supportSite`          | string        | Support site URL                           |
| `marketingSite`        | string        | Marketing site URL                         |
| `donationUrl`          | string?       | Optional donation URL                      |
| `docsUrl`              | string?       | Optional documentation URL                 |
| `description`          | object        | Short and long descriptions (localized)    |
| `images`               | object        | Image configurations by image ID           |
| `volumes`              | string[]      | Volume IDs for persistent data             |
| `dependencies`         | object        | Package dependencies                       |
| `hardwareRequirements` | object        | Hardware requirements (arch, RAM, devices) |
| `hardwareAcceleration` | boolean       | Whether package uses hardware acceleration |
| `gitHash`              | string?       | Git commit hash                            |
| `osVersion`            | string        | Minimum StartOS version                    |
| `sdkVersion`           | string?       | SDK version used to build                  |

### javascript.squashfs

Contains the package JavaScript that implements the `ABI` interface from `@start9labs/start-core` (re-exported via `@start9labs/start-sdk`). This code runs in the container runtime and manages the package lifecycle.

The squashfs is mounted at `/usr/lib/startos/package/` and the runtime loads `index.js`.

### images/

Container images organized by architecture:

- **`<image-id>.squashfs`** - Container root filesystem
- **`<image-id>.json`** - Image metadata (entrypoint, user, workdir, etc.)
- **`<image-id>.env`** - Environment variables for the container

Images are built from Docker/Podman and converted to squashfs. The `ImageConfig` in manifest specifies:

- `arch` - Supported architectures
- `emulateMissingAs` - Fallback architecture for emulation
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
start-cli s9pk pack <manifest-path> -o <output.s9pk>
```

Images can be sourced from:

- Docker/Podman build (`--docker-build`)
- Existing Docker tag (`--docker-tag`)
- Pre-built squashfs files

## Related Code

- `core/src/s9pk/v2/mod.rs` - S9pk struct and serialization
- `core/src/s9pk/v2/manifest.rs` - Manifest types
- `core/src/s9pk/v2/pack.rs` - Packing logic
- `core/src/s9pk/merkle_archive/` - Merkle archive implementation
