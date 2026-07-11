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
npm --prefix projects/start-os/container-runtime run build      # clean + tsc -> dist/
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

The root prettier config (`.prettierrc.json` at the repo root) governs this directory — there is no local config or format script. Format from the monorepo root:

```bash
make web-format               # prettier --write over the whole repo
make web-format-check         # CI read-only check
```

Never run prettier from inside this directory: the root `.prettierignore` only applies when prettier runs from the root cwd, and it must apply — it protects `__fixtures__/` (byte-exact test fixtures) from being reformatted.
