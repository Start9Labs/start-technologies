# AGENTS.md — Container Runtime

Node.js/TypeScript service runtime that runs inside StartOS package LXC containers. Sub-component of the **start-os** product. `CLAUDE.md` is a one-line `@AGENTS.md` import — edit this file, not `CLAUDE.md`. Wire-protocol reference: [RPCSpec.md](RPCSpec.md); structure: [ARCHITECTURE.md](ARCHITECTURE.md); contributor workflow: [CONTRIBUTING.md](CONTRIBUTING.md).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/index.ts` — entry; wires `getSystem` into `RpcListener`.
- `src/Adapters/RpcListener.ts` — JSON-RPC dispatch on `service.sock`.
- `src/Adapters/EffectCreator.ts` — builds effects passed to procedures.
- `src/Adapters/Systems/` — `SystemForStartOs` (current) + `SystemForEmbassy` (legacy compat: manifest/config-spec transforms, effect polyfills).
- `src/Interfaces/`, `src/Models/` — contracts and domain types.

## Build & test (run from the repo root)

Run from the monorepo root (`/path/to/start-os`):

```bash
cd projects/start-sdk && make bundle && cd -                  # build SDK dependency first
npm --prefix projects/start-os/container-runtime ci           # install deps
npm --prefix projects/start-os/container-runtime run check     # tsc --noEmit (type-check)
npm --prefix projects/start-os/container-runtime run build      # clean + tsc -> dist/
npm --prefix projects/start-os/container-runtime test           # jest (ts-jest)
make container-runtime-test                            # SDK + jest via top-level Makefile
```

Tests are Jest + `ts-jest` (`jest.config.js`, `rootDir: ./src`). `mime` is mocked via `__mocks__/mime.js`. Place tests next to the code as `*.test.ts`; `SystemForEmbassy` carries snapshot tests (`__snapshots__/`) and fixtures (`__fixtures__/`).

## Gotchas

- **Depends on the _built_ SDK at `../../start-sdk/dist`** (declared in `package.json` as `"@start9labs/start-sdk": "file:../../start-sdk/dist"`) **and on `@start9labs/start-core` at `../../../shared-libs/ts-modules/start-core/dist`** (for what were the deep `base/lib/...` imports). Editing `projects/start-sdk/` or `shared-libs/ts-modules/start-core/` source alone has no effect here — rebuild first: `cd projects/start-sdk && make bundle` (which builds start-core and bundles it). The Makefile target `projects/start-os/container-runtime/package-lock.json` also depends on `projects/start-sdk/dist/package.json`, so a stale SDK can break `npm ci`/`check`/`test`.
- **Formatting is the root prettier config** (`.prettierrc.json` at the repo root), applied via `make format` / `make web-format` from the repo root. Never run prettier from inside this directory: the root `.prettierignore` (which protects `__fixtures__/`) only applies when prettier runs from the root cwd.
- **`CLAUDE.md` is just `@AGENTS.md`** — edit this file, not `CLAUDE.md`.

## Image build (gotchas)

- Compiled JS is installed into the container at `/usr/lib/startos/init/index.js` (the systemd unit runs `start-container pipe-wrap node … /usr/lib/startos/init/index.js`).
- `update-image-local.sh` mounts the **repo root** into `start9/build-env` (at `/root/start-os`) and runs `update-image.sh` inside it. `update-image.sh` `cd`s to its own dir (`projects/start-os/container-runtime/`), so it reaches the repo-root build output three levels up — it copies `start-container` from `../../../target/<arch>-unknown-linux-musl/release/`.
- The squashfs lands at `rootfs.<arch>.squashfs` and is installed to `/usr/lib/startos/container-runtime/rootfs.squashfs`.

## Stale-path note (monorepo)

Pre-monorepo docs referenced `core/`, `sdk/`, `web/`, `patch-db/`, `container-runtime/` at the repo root. Current locations: host lib `shared-libs/crates/start-core`, SDK `projects/start-sdk`, Angular `shared-libs/ts-modules` + product `web/` dirs, this runtime `projects/start-os/container-runtime`, first-party `shared-libs/crates/patch-db`.
