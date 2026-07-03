# Contributing to Container Runtime

For general environment setup (Node, repo clone, monorepo layout), see the root [CONTRIBUTING.md](../../../CONTRIBUTING.md). This file covers only what's specific to the container runtime.

## Documentation

This sub-tree's docs are:

- `README.md` — what this is + quickstart
- `ARCHITECTURE.md` — how it's structured (modules, RPC boundary, image build)
- `AGENTS.md` — agent/dev operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import)
- `CONTRIBUTING.md` — this file
- `RPCSpec.md` — the JSON-RPC wire protocol

## Prerequisites

- Node.js v20+ (see root CONTRIBUTING for the recommended version pin).
- The **built** SDK at `../../start-sdk/dist` and **built** `@start9labs/start-core` at `../../../shared-libs/ts-modules/start-core/dist`. `package.json` references both via `file:` deps, so build them whenever they change:

  ```bash
  cd projects/start-sdk && make bundle && cd -      # builds @start9labs/start-core and bundles it into the SDK dist
  ```

## Building

Run from the monorepo root:

```bash
npm --prefix projects/start-os/container-runtime ci          # install deps
npm --prefix projects/start-os/container-runtime run check     # type-check (tsc --noEmit)
npm --prefix projects/start-os/container-runtime run build      # prettier + clean + tsc -> dist/
```

Or `cd projects/start-os/container-runtime` first and drop the `--prefix`.

## Testing

Run from the monorepo root:

```bash
npm --prefix projects/start-os/container-runtime test           # jest
make container-runtime-test                            # build SDK + run jest via Makefile
```

Jest with `ts-jest` (`jest.config.js`, `rootDir: ./src`). The `mime` module is mocked via `__mocks__/mime.js`. Place tests next to the code they cover with the `.test.ts` extension. `SystemForEmbassy` uses snapshot tests (`__snapshots__/`) and fixtures (`__fixtures__/`) — update snapshots deliberately, not blindly.

## Formatting

Prettier config lives in `package.json`:

- `trailingComma: "all"`, `tabWidth: 2`, `semi: false`, `singleQuote: false`.

The runtime uses **double quotes**, unlike `start-sdk` and `shared-libs/ts-modules` (single quotes there) — do not normalize. `npm run build` runs Prettier `--write` before compiling, so formatting is applied automatically during a build.

From the monorepo root, the canonical formatter is the top-level make target (container-runtime folds into the start-os product):

```bash
make start-os-format          # write formatting
make start-os-format-check    # CI read-only check
```
