# Container Runtime Architecture

The container runtime is a long-lived Node.js/TypeScript process that runs inside every StartOS service container. It loads the service's JavaScript from the s9pk, manages subcontainers, and serves a JSON-RPC API that the StartOS host daemon (`shared-libs/crates/start-core`) drives over a Unix socket.

```
LXC Container (uniform base image for all services)
└── systemd
    └── container-runtime.service
        └── start-container pipe-wrap node /usr/lib/startos/init/index.js
            └── Loads package JS (from s9pk javascript.squashfs)
                └── Package JS launches subcontainers (from images in s9pk)
```

The `start-container` binary (built from package `start-os`, see `shared-libs/crates/start-core`) supervises the Node process and wraps its stdio. Package JavaScript must export functions conforming to the `ABI` type defined in `shared-libs/ts-modules/start-core/lib/types.ts` (package `@start9labs/start-core`); the runtime imports `@start9labs/start-sdk` from the built SDK at `../../start-sdk/dist` and `@start9labs/start-core` from `../../../shared-libs/ts-modules/start-core/dist`.

## Place in the monorepo

This runtime lives at `projects/start-os/container-runtime` and is a Node.js package with its own `package.json` (built via `npm --prefix projects/start-os/container-runtime`, not the Angular workspace). It is consumed by the StartOS host daemon `start_core` (`shared-libs/crates/start-core`), which drives it over a Unix socket; the supervising `start-container` binary is built from package `start-os`, and its runtime API comes from `@start9labs/start-sdk` (`projects/start-sdk`).

## Source layout (`src/`)

| Path                        | Responsibility                                                                                                                     |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `index.ts`                  | Entry point — wires `getSystem` into the `RpcListener` and starts it                                                               |
| `Adapters/RpcListener.ts`   | Listens on `service.sock`, dispatches JSON-RPC methods                                                                             |
| `Adapters/EffectCreator.ts` | Builds the effects object passed to package procedures                                                                             |
| `Adapters/Systems/`         | System backends: `SystemForStartOs` (current) + `SystemForEmbassy` (legacy compat shim, including config-spec/manifest transforms) |
| `Interfaces/`               | `System`, `GetDependency`, `AllGetDependencies` contracts                                                                          |
| `Models/`                   | Domain types — `Effects`, `CallbackHolder`, `Volume`, `DockerProcedure`, `JsonPath`, `Duration`                                    |

`SystemForEmbassy` carries the bulk of the legacy-package compatibility logic (manifest matching, config-spec transformation, effect polyfills) and ships with fixtures and snapshot/unit tests.

## `/media/startos/` directory (mounted by the host into the container)

| Path                 | Description                                           |
| -------------------- | ----------------------------------------------------- |
| `volumes/<name>/`    | Package data volumes (id-mapped, persistent)          |
| `assets/`            | Read-only assets from s9pk `assets.squashfs`          |
| `images/<name>/`     | Container images (squashfs, used for subcontainers)   |
| `images/<name>.env`  | Environment variables for image                       |
| `images/<name>.json` | Image metadata                                        |
| `backup/`            | Backup mount point (mounted during backup operations) |
| `rpc/service.sock`   | RPC socket (container runtime listens here)           |
| `rpc/host.sock`      | Host RPC socket (effects callbacks back to host)      |

## RPC protocol

The runtime listens on `/media/startos/rpc/service.sock` and exposes a JSON-RPC API for the host (`init`, `exit`, `start`, `stop`, `execute`, `callback`, …). Effects flowing the other way (filesystem, network, store, dependency calls) reach the host over `host.sock`. Backup execution returns optional transfer measurements from the package to the host. The full method list and parameter types live in [`RPCSpec.md`](RPCSpec.md).

## Image build

The runtime ships as a squashfs rootfs baked into the StartOS image:

1. `download-base-image.sh` fetches the Debian trixie LXC base (`debian.<arch>.squashfs`).
2. `npm run build` compiles `src/` to `dist/`; `install-dist-deps.sh` installs production deps into `dist/node_modules`.
3. `update-image-local.sh` runs `update-image.sh` inside the `start9/build-env` container: it overlays `dist/` into the base at `/usr/lib/startos/init/`, drops in `start-container` and the systemd units, runs `deb-install.sh` in the chroot, and mksquashfs's the result to `rootfs.<arch>.squashfs`.

The top-level `Makefile` orchestrates all of this (`projects/start-os/container-runtime/rootfs.$(ARCH).squashfs` target); the squashfs is installed to `/usr/lib/startos/container-runtime/rootfs.squashfs` in the final OS image.

## S9PK structure

The s9pk format determines what the runtime loads at startup. Its definition and tooling live in `shared-libs/crates/start-core/src/s9pk` (host side) and `start-sdk` (packaging side).

## Further reading

- [README.md](README.md) — what this is + quickstart
- [AGENTS.md](AGENTS.md) — local build, type-check, and test workflow
- [RPCSpec.md](RPCSpec.md) — full JSON-RPC wire protocol
