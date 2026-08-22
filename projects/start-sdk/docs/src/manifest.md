# Manifest

The manifest defines service identity, metadata, and build configuration. It lives in `startos/manifest/` as two files:

- `index.ts` -- the `setupManifest()` call
- `i18n.ts` -- translated strings for `description`

## manifest/i18n.ts

Locale objects for user-facing manifest strings. Each is a record of locale to string:

```typescript
export const short = {
  en_US: 'Brief description (one line)',
  es_ES: 'Descripcion breve (una linea)',
  de_DE: 'Kurze Beschreibung (eine Zeile)',
  pl_PL: 'Krotki opis (jedna linia)',
  fr_FR: 'Description breve (une ligne)',
}

export const long = {
  en_US: 'Longer description explaining what the service does and its key features.',
  es_ES: 'Descripcion mas larga que explica que hace el servicio y sus caracteristicas principales.',
  de_DE: 'Langere Beschreibung, die erklart, was der Dienst tut und seine wichtigsten Funktionen.',
  pl_PL: 'Dluzszy opis wyjasniajacy, co robi usluga i jej kluczowe funkcje.',
  fr_FR: 'Description plus longue expliquant ce que fait le service et ses fonctionnalites principales.',
}
```

### How long each one may be

|         | limit           |
| ------- | --------------- |
| `short` | 120 characters  |
| `long`  | 2000 characters |

Every locale gets the same limit, and it is enforced when the package is
validated, so a description that overruns fails the build.

`short` is not truncated when it overruns — the marketplace tile clamps it to
two lines with `overflow: hidden`, so the remainder is silently cut off in the
one place users browse. **Two lines is about 80 characters**, and it is the same
two lines whatever language the tile renders in. The limit sits above that so a
translation isn't failed over a few characters' growth — German and Polish
routinely run 20-30% longer than the same English sentence — not as permission
to run long. Write the English well inside 80 and every locale still fits the
tile.

Two more characters' worth of advice:

- **Don't open with the service's name.** The tile renders the title in bold on
  the line directly above, so "Foo is a self-hosted bar" spends its first words
  on something the reader can already see.
- **Say what it is, not what it is like.** Comparisons and superlatives cost
  more characters than they earn at this size.

`long` has no such budget — it is rendered unclamped on the service's details
page — so its limit is about the reader's patience rather than the layout.

## manifest/index.ts

```typescript
import { setupManifest } from '@start9labs/start-sdk'
import { short, long } from './i18n'

export const manifest = setupManifest({
  id: 'my-service',
  title: 'My Service',
  license: 'MIT',
  packageRepo: 'https://github.com/Start9Labs/my-service-startos',
  upstreamRepo: 'https://github.com/original/my-service',
  marketingUrl: 'https://example.com/',
  donationUrl: null,
  description: { short, long },
  volumes: ['main'],
  images: {
    /* see Images Configuration below */
  },
  dependencies: {},
})
```

## Required Fields

| Field               | Description                                                                              |
| ------------------- | ---------------------------------------------------------------------------------------- |
| `id`                | Unique identifier (lowercase, hyphens allowed; `start-os` is reserved for the OS itself) |
| `title`             | Display name shown in UI                                                                 |
| `license`           | SPDX identifier (`MIT`, `Apache-2.0`, `GPL-3.0`, etc.)                                   |
| `packageRepo`       | URL to the StartOS package repository                                                    |
| `upstreamRepo`      | URL to the original project repository                                                   |
| `marketingUrl`      | URL for the project's main website                                                       |
| `donationUrl`       | Donation URL or `null`                                                                   |
| `description.short` | Locale object (see `manifest/i18n.ts`)                                                   |
| `description.long`  | Locale object (see `manifest/i18n.ts`)                                                   |
| `volumes`           | Storage volumes (usually `['main']`)                                                     |
| `images`            | Docker image configuration (including `arch`)                                            |
| `dependencies`      | Service dependencies                                                                     |

## License

Check the upstream project's LICENSE file and use the correct SPDX identifier (e.g., `MIT`, `Apache-2.0`, `GPL-3.0`). If you have a git submodule, symlink to its license. Otherwise, copy the license text directly from the upstream repository:

```bash
# With submodule
ln -sf upstream-project/LICENSE LICENSE

# Without submodule -- copy from upstream repo
```

## Icon

Symlink from upstream if available (svg, png, jpg, or webp, max 40 KiB):

```bash
ln -sf upstream-project/logo.svg icon.svg
```

## Images Configuration

Each image can include an `arch` field specifying supported architectures. It defaults to `['x86_64', 'aarch64', 'riscv64']` if omitted, but it is good practice to list architectures explicitly for transparency. The `arch` field must align with the `ARCHES` variable in the Makefile.

### Pre-built Docker Tag

Use when an image exists on Docker Hub or another registry:

```typescript
images: {
  main: {
    source: {
      dockerTag: 'nginx:1.25',
    },
    arch: ['x86_64', 'aarch64'],
  },
},
```

### Local Docker Build

Use when building from a Dockerfile in the project:

```typescript
// Dockerfile in project root
images: {
  main: {
    source: {
      dockerBuild: {},
    },
    arch: ['x86_64', 'aarch64'],
  },
},
```

**If upstream has a working Dockerfile**: Set `workdir` to the upstream directory. If the Dockerfile is named `Dockerfile`, you can omit the `dockerfile` field:

```typescript
images: {
  main: {
    source: {
      dockerBuild: {
        workdir: './upstream-project',
      },
    },
    arch: ['x86_64', 'aarch64'],
  },
},
```

For a non-standard Dockerfile name, specify `dockerfile` relative to project root:

```typescript
images: {
  main: {
    source: {
      dockerBuild: {
        workdir: './upstream-project',
        dockerfile: './upstream-project/sync-server.Dockerfile',
      },
    },
    arch: ['x86_64', 'aarch64'],
  },
},
```

**If you need a custom Dockerfile**: Create one in your project root:

```dockerfile
COPY upstream-project/ .
```

### Architecture Support

The `arch` field accepts these values:

| Value     | Architecture     |
| --------- | ---------------- |
| `x86_64`  | Intel/AMD 64-bit |
| `aarch64` | ARM 64-bit       |
| `riscv64` | RISC-V 64-bit    |

Most services support `['x86_64', 'aarch64']`. Only add `riscv64` if the upstream image actually supports it. The `ARCHES` variable in the Makefile must align (see [Makefile](./makefile.md)).

### GPU/Hardware Acceleration

For services requiring GPU access:

```typescript
images: {
  main: {
    source: {
      dockerTag: 'ollama/ollama:0.13.5',
    },
    arch: ['x86_64', 'aarch64'],
    nvidiaContainer: true,  // Enable NVIDIA GPU support
  },
},
hardwareAcceleration: true,  // Top-level flag
```

#### Hardware requirements and variants

A package that targets several accelerators (NVIDIA, AMD, CPU-only, …) ships one **variant** per accelerator: a separate `.s9pk` built with a different `VARIANT` in the Makefile (see [Makefile](./makefile.md)), all published under a single version. The manifest reads `process.env.VARIANT` to pick per-variant settings, including `hardwareRequirements.device` — a list of device filters telling StartOS which hardware a variant needs:

```typescript
const variant = process.env.VARIANT || 'cpu'

// inside setupManifest({ ... })
hardwareRequirements: {
  device:
    variant === 'nvidia'
      ? [{ class: 'display', product: null, vendor: null, driver: 'nvidia', description: 'An NVIDIA GPU' }]
      : variant === 'rocm'
        ? [{ class: 'display', product: null, vendor: null, driver: 'amdgpu', description: 'An AMD GPU' }]
        : [], // cpu: runs anywhere
},
```

The registry stores a version's variants together and disambiguates them **by hardware requirement** — on a given machine StartOS offers the variant whose requirement the detected hardware satisfies.

> [!WARNING]
> Every variant must declare a **distinct** hardware requirement, and **at most one** variant may have an empty requirement (`[]`, the catch-all fallback). Two variants presenting the same requirement — most often two with an empty `device` array — collide when the second is published, and the registry rejects it:
>
> ```
> Invalid Request: package.add: package metadata mismatch: remove the existing version first, then re-add
> ```
>
> In particular an `nvidia` variant must carry an NVIDIA `device` filter, not `[]` — `nvidiaContainer: true` wires up the GPU runtime but does **not** set a hardware requirement, so without the filter the NVIDIA variant is indistinguishable from the CPU fallback and one of the two fails to publish.

#### Minimum RAM

`hardwareRequirements.ram` is the memory floor below which StartOS will not offer the package. **It is compared against the host's total RAM in bytes.** StartOS records `MemTotal` in bytes and the check is a raw comparison against the number you declare — nothing in the SDK or the OS converts units on your behalf.

```typescript
hardwareRequirements: {
  ram: 8 * 1024 ** 3, // 8 GiB
},
```

> [!WARNING]
> A value that reads as megabytes — `ram: 8192` — declares **8 KiB**, which every machine satisfies, so the requirement silently gates nothing. Write the byte count as an explicit power-of-two expression, so the unit is visible where the value is.

Leave it unset when the service has no hard floor. Bear in mind that a box failing the check is not offered the package at all, so **raising the floor on an already-published package cuts existing installs below it off from further updates** — call that out in the release notes when you do it.

### Virtual Networking (VPN / kernel tun interfaces)

For services that bring up their own kernel tunnel interface — VPNs, WireGuard, or any `tun`-class workload — set `virtualNetworking: true` at the manifest top level:

```typescript
virtualNetworking: true,
```

When set, StartOS exposes `/dev/net/tun` inside the service's container, so the service can create and configure tunnel interfaces. The flag grants that device and nothing else; it does not change the container's capabilities. Enable it only when the service genuinely needs a kernel tunnel interface.

### Nested OCI Runtimes (Docker / Podman inside a service)

For services that need to run their own OCI containers — e.g. CI runners like `gitea-act-runner` that spawn build containers per job — set both `userspaceFilesystems` and `virtualNetworking` at the manifest top level:

```typescript
userspaceFilesystems: true,  // /dev/fuse for fuse-overlayfs storage
virtualNetworking: true,     // /dev/net/tun for slirp4netns / pasta networking
```

`userspaceFilesystems` exposes `/dev/fuse` so a rootless engine (Podman or Docker) can use `fuse-overlayfs` for layered storage. `virtualNetworking` exposes `/dev/net/tun` so it can use `slirp4netns` (or `pasta`) for networking. Both are opt-in. Service authors are still responsible for installing the OCI engine in the image and configuring it for rootless mode — see [Run a Nested OCI Runtime](./recipe-nested-oci-runtime.md) for the full recipe (subuid setup, daemon configuration, and the runc wrapper required when using Docker).

### Hardware Virtualization (KVM)

For services that run their own virtual machines — QEMU/KVM, Firecracker, or a device emulator such as the Android Emulator — set `hardwareVirtualization: true` at the manifest top level:

```typescript
hardwareVirtualization: true,
```

When set, StartOS exposes `/dev/kvm` inside the service's container, so the guest runs on the CPU's virtualization extensions instead of being interpreted in software. It grants the device and nothing else: the service stays unprivileged, user-namespace mapped, and AppArmor-confined.

> [!IMPORTANT]
> The granted node belongs to the container's root and carries the permissions the server gives it, which on `/dev/kvm` are `0660`. **Run the process that opens it as root**, as the GPU packages do for `hardwareAcceleration` — that is the only arrangement StartOS guarantees.

The device appears only on a server whose CPU supports virtualization and whose kernel has KVM active for it. Where it does not, the service starts as normal with no `/dev/kvm` — so a service that can fall back to software emulation should test for the device and do so, and one that cannot should declare a health check saying this server does not support KVM. Give that check `gracePeriod: 0`, so it reports the reason instead of `starting`, and a `cooldownTrigger` — the default re-polls a failing check every second for the life of the service. See [Health Checks](./main.md#health-checks).

> [!WARNING]
> `/dev/kvm` is a direct interface to the host kernel's hypervisor, so it widens the kernel attack surface reachable from the service. Enable it only for a service that genuinely runs virtual machines.

### Multiple Images

Services can define multiple images. Each image needs its own `arch` field:

```typescript
images: {
  app: {
    source: { dockerTag: 'myapp:latest' },
    arch: ['x86_64', 'aarch64'],
  },
  db: {
    source: { dockerTag: 'postgres:15' },
    arch: ['x86_64', 'aarch64'],
  },
},
```

## Volumes

Storage volumes for persistent data. When possible, prefer matching the upstream project's volume naming convention for clarity:

```typescript
// If upstream docker-compose uses a volume named "mcaptcha-data"
volumes: ['mcaptcha-data'],

// Simple services can use 'main'
volumes: ['main'],
```

For services needing separate storage areas:

```typescript
volumes: ['main', 'db', 'config'],
```

Reference these in `main.ts` mounts by the volume ID you chose.

## Dependencies

Declare dependencies on other StartOS services. Note that dependency `description` is a plain string, not a locale object:

```typescript
dependencies: {
  // Required dependency
  bitcoin: {
    description: 'Required for blockchain data',
    optional: false,
  },

  // Optional dependency with metadata
  'c-lightning': {
    description: 'Needed for Lightning payments',
    optional: true,
    metadata: {
      title: 'Core Lightning',
      icon: 'https://raw.githubusercontent.com/Start9Labs/cln-startos/refs/heads/master/icon.png',
    },
  },
},
```
