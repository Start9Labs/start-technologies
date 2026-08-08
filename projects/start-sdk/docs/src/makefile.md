# Makefile Build System

A StartOS package's `Makefile` carries only project-specific configuration and includes the shared build logic (`s9pk.mk`) that ships inside the SDK.

## File Structure

```
my-service-startos/
└── Makefile     # Project-specific config; includes the SDK's s9pk.mk
```

## s9pk.mk

The `s9pk.mk` file contains all the common build logic shared across StartOS packages. It ships **inside the published SDK** (`@start9labs/start-sdk`), so your `Makefile` includes it straight from `node_modules` — there's nothing to vendor or copy into the package, and bumping the SDK delivers build-system fixes automatically.

### Targets

| Target               | Description                                          |
| -------------------- | ---------------------------------------------------- |
| `make` or `make all` | Build for all architectures (default)                |
| `make x86`           | Build for x86_64 only                                |
| `make arm`           | Build for aarch64 only                               |
| `make riscv`         | Build for riscv64 only                               |
| `make universal`     | Build a single package containing all architectures  |
| `make install`       | Install the most recent .s9pk to your StartOS server |
| `make clean`         | Remove build artifacts                               |

### Variables

| Variable  | Default         | Description                              |
| --------- | --------------- | ---------------------------------------- |
| `ARCHES`  | `x86 arm riscv` | Architectures to build by default        |
| `TARGETS` | `arches`        | Default build target                     |
| `VARIANT` | (unset)         | Optional variant suffix for package name |

## Makefile

The project `Makefile` is minimal and just includes `s9pk.mk`:

```makefile
include node_modules/@start9labs/start-sdk/s9pk.mk
```

### Adding Custom Targets

For services with variants (e.g., GPU support), extend the Makefile:

```makefile
TARGETS := generic rocm
ARCHES := x86 arm

include node_modules/@start9labs/start-sdk/s9pk.mk

.PHONY: generic rocm

generic:
	$(MAKE) all_arches VARIANT=generic

rocm:
	ROCM=1 $(MAKE) all_arches VARIANT=rocm ARCHES=x86_64
```

This produces packages named `myservice_generic_x86_64.s9pk` and `myservice_rocm_x86_64.s9pk`.

> [!WARNING]
> Each variant must declare a **distinct** hardware requirement in the manifest (with at most one empty fallback), or publishing the second variant fails with a registry metadata mismatch. See [GPU/Hardware Acceleration](./manifest.md#hardware-requirements-and-variants).

### Overriding Defaults

Override variables _before_ `include node_modules/@start9labs/start-sdk/s9pk.mk`:

```makefile
# Build only for x86 and arm
ARCHES := x86 arm

include node_modules/@start9labs/start-sdk/s9pk.mk
```

## Build Commands

```bash
# Build for all architectures
make

# Build for a specific architecture
make x86
make arm

# Install to StartOS server (requires a workspace whose .startos/config.yaml points at your device)
make install

# Clean build artifacts
make clean
```

`start-cli s9pk pack` adds read access to every file and read/search access to every directory in
the packaged JavaScript ingredient (`ugo+rX`). This keeps `javascript.squashfs` usable when Node.js
or another build tool created `javascript/` under a restrictive umask. Existing executable bits are
preserved, and the source directory's permissions are not changed.

### Chaining Commands

You can chain multiple targets in a single invocation:

```bash
make clean arm                    # Clean, then build ARM package
make clean x86 install            # Clean, build x86 package, then install
make clean universal install      # Clean, build universal, then install
```

## Prerequisites

Building signs the package with your **workspace signing key**, so the package must live inside a packaging workspace. If you haven't created one yet, do that first — see [Environment Setup — Set Up Your Packaging Workspace](./environment-setup.md#set-up-your-packaging-workspace). Running `make` without a workspace fails with a message telling you to run `start-cli s9pk init-workspace`.

The build also needs the tools from [Environment Setup](./environment-setup.md) — Docker (running), `make`, Node.js/`npm`, `start-cli`, `git`, and `jq`.

## Installation

`make install` builds nothing on its own — it uploads the most recently built `.s9pk` to a StartOS device, so build first (for development, just your device's architecture — `make x86` or `make arm`). It resolves the device from your workspace `.startos/config.yaml` (the `host.default` profile) or an explicit `-H`.

1. Point your workspace at the device. Edit `.startos/config.yaml` (at the workspace root, **not** `~/.startos/config.yaml`) so `host.default` is your device's address:

   ```yaml
   host:
     default: https://your-device.local
   ```

2. Log in once. `start-cli` needs a session on the device:

   ```sh
   start-cli auth login
   ```

   Enter your StartOS master password when prompted.

3. Build and install for your device's architecture (`x86` or `arm`) — the fast path for development:

   ```sh
   make x86 install       # or: make arm install
   ```

   (`make install` on its own installs the most recent build. Reach for `make universal`
   only when publishing — building all architectures is slower and unnecessary for local
   testing.)

> [!NOTE]
> `make install` talks to the device over HTTPS, so your computer must trust the device's certificate — the same trust you set up to open its web interface in a browser. If it isn't trusted yet, import the device's root CA into your system trust store; or, for a one-off, sideload the `.s9pk` through the web interface instead (see [Sideloading](/start-os/sideloading.html)), which needs no certificate setup.
>
> To install to a device other than `host.default`, run `start-cli` directly with `-H` (a profile name or URL): `start-cli -H prod package install -s <your-package>.s9pk`.

### Example Output

**Building an ARM package:**

```
$ make arm
   Re-evaluating ingredients...
   Packing 'albyhub_aarch64.s9pk'...
Build Complete!

  Alby Hub   v1.19.3:1
  Filename:   albyhub_aarch64.s9pk
  Size:       7M
  Arch:       aarch64
  SDK:        0.4.0-beta.36
  Git:        78c30ec776f6a9d55be3701e9b82093c866a382c
```

> [!NOTE]
> If you have uncommitted changes, the Git hash will be shown in red.

**Installing a package:**

```
$ make arm install

Installing to working-finalist.local ...
Sideloading 100%
  Uploading...
  Validating Headers...
  Unpacking...
```
