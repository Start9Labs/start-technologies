# Contributing to Start SDK

This guide covers developing the SDK itself. If you're building a service package _using_ the SDK, see the [packaging docs](https://docs.start9.com/packaging).

The SDK lives at `projects/start-sdk/` inside the [start-technologies monorepo](../../README.md). For contributing to the broader StartOS project, see the root [CONTRIBUTING.md](../../CONTRIBUTING.md) and root [AGENTS.md](../../AGENTS.md).

## Documentation

This sub-tree's docs:

- `README.md` — what this is + quickstart
- `ARCHITECTURE.md` — how it's structured (modules, data flow)
- `CONTRIBUTING.md` — this file; how to build/test/contribute
- `AGENTS.md` — practical agent/dev instructions (`CLAUDE.md` is just `@AGENTS.md`)
- `CHANGELOG.md` — Keep a Changelog style release history

The developer-facing reference is the packaging mdbook in `docs/` (published at [docs.start9.com/packaging](https://docs.start9.com/packaging)).

## Prerequisites

- **Node.js v22+** (via [nvm](https://github.com/nvm-sh/nvm) recommended)
- **npm** (ships with Node.js)
- **GNU Make**

Verify your setup:

```bash
node --version   # v22.x or higher
npm --version
make --version
```

## Repository Layout

```
start-sdk/                 # @start9labs/start-sdk (full developer-facing SDK)
├── lib/               #   TypeScript source
├── dist/              # Build output (generated, published to npm; bundles @start9labs/start-core)
├── docs/              # "Service Packaging" mdbook (docs.start9.com/packaging)
├── package.json
├── tsconfig.json
├── jest.config.js
├── s9pk.mk            # Build plumbing shipped in the published package
├── tsconfig.base.json # tsconfig shipped in the published package
├── Makefile           # Build orchestration
├── README.md
├── ARCHITECTURE.md
├── CHANGELOG.md
└── AGENTS.md
```

The foundational types, ABI, effects, and OS bindings live in the separate `@start9labs/start-core` lib at `shared-libs/ts-modules/start-core/`; the SDK imports it and its `dist/` bundles it (npm `bundleDependencies`).

`s9pk.mk` and `tsconfig.base.json` are copied into `dist/` so service packages can `include node_modules/@start9labs/start-sdk/s9pk.mk` and `extends "@start9labs/start-sdk/tsconfig.base.json"` — they are a public contract; editing them changes every package's build.

## Getting Started

From `projects/start-sdk/`, install dependencies:

```bash
make node_modules
```

This runs `npm ci` (the start-core lib it bundles is built separately via `shared-libs/ts-modules/start-core`'s own Makefile).

## Building

### Full Build

```bash
make bundle
```

This runs the complete pipeline: build `@start9labs/start-core` (prerequisite), TypeScript compilation, hand-written pair copying, node_modules bundling, formatting, and tests. The output lands in `dist/` (with start-core bundled in).

### Individual Targets

| Target        | Description                                       |
| ------------- | ------------------------------------------------- |
| `make bundle` | Full build: compile + format + test               |
| `make dist`   | Compile the SDK (depends on the built start-core) |
| `make fmt`    | Run Prettier on all `.ts` files                   |
| `make check`  | Type-check without emitting                       |
| `make clean`  | Remove all build artifacts and node_modules       |

To build the bundled start-core lib on its own: `cd shared-libs/ts-modules/start-core && make dist`.

### What the Build Does

1. **Peggy parser generation** — `shared-libs/ts-modules/start-core/lib/exver/exver.pegjs` is compiled to `exver.ts` (the ExVer version parser) when start-core builds
2. **TypeScript compilation** — Strict mode, CommonJS output, declaration files
   - `@start9labs/start-core` compiles to `shared-libs/ts-modules/start-core/dist/`
   - the SDK compiles to `dist/`
3. **Hand-written pair copying** — `.js`/`.d.ts` files without a corresponding `.ts` source are copied into the output directories. These are manually maintained JavaScript files with hand-written type declarations.
4. **Dependency bundling** — `node_modules/` (including the bundled `@start9labs/start-core`) is rsynced into the output directory so the published package is self-contained
5. **Formatting** — Prettier formats all TypeScript source
6. **Testing** — Jest runs both test suites

## Testing

```bash
# Run the SDK tests
make test

# Run a specific test file directly
npx jest --testPathPattern=host
```

The start-core lib has its own test suite: `cd shared-libs/ts-modules/start-core && make test` (or `npx jest --testPathPattern=exver` there).

Tests use [Jest](https://jestjs.io/) with [ts-jest](https://kulshekhar.github.io/ts-jest/) for TypeScript support. Configuration is in each package's `jest.config.js`.

### Test Files

Tests live alongside their subjects or in dedicated `test/` directories:

- `shared-libs/ts-modules/start-core/lib/test/` — ExVer parsing, input spec types, deep merge, graph utilities, type validation
- `shared-libs/ts-modules/start-core/lib/util/inMs.test.ts` — Time conversion utility
- `lib/test/` — Health checks, host binding, input spec builder

Test files use the `.test.ts` extension and are excluded from compilation via `tsconfig.json`.

## Formatting

```bash
make fmt
```

Runs Prettier with the project config (single quotes, no semicolons, trailing commas, 2-space indent). The Prettier config lives in each sub-package's `package.json`:

```json
{
  "trailingComma": "all",
  "tabWidth": 2,
  "semi": false,
  "singleQuote": true
}
```

## Type Checking

To check types without building:

```bash
make check
```

Or directly:

```bash
npm run check                                            # the SDK
cd ../../shared-libs/ts-modules/start-core && npm run check   # start-core
```

Both packages use strict TypeScript (`"strict": true`) targeting ES2021 with CommonJS module output.

## Local Development with a Service Package

To test SDK changes against a local service package without publishing to npm:

```bash
# Build and create a local npm link
make link

# In your service package directory
npm link @start9labs/start-sdk
```

This symlinks the built `dist/` into your global node_modules so your service package picks up local changes.

## Publishing

```bash
make publish
```

This builds the full bundle, then runs `npm publish --access=public --tag=latest` from `dist/`. The published package is `@start9labs/start-sdk`.

Only the `dist/` directory is published — it contains the compiled JavaScript, declaration files, bundled dependencies, and package metadata.

## Adding New Features

### start-core vs SDK

Decide where new code belongs:

- **`@start9labs/start-core`** (`shared-libs/ts-modules/start-core/`) — Types, interfaces, ABI contracts, OS bindings, and low-level builders that have no dependency on the SDK layer. Code here should be usable independently.
- **SDK** (`projects/start-sdk/lib/`) — Developer-facing API, convenience wrappers, runtime helpers (daemons, health checks, backups, file helpers, subcontainers). Code here imports from `@start9labs/start-core` and adds higher-level abstractions.

### Key Conventions

- **Builder pattern** — Most APIs use immutable builder chains (`.addDaemon()`, `.mountVolume()`, `.addAction()`). Each call returns a new type that accumulates configuration.
- **Effects boundary** — All runtime interactions go through the `Effects` interface. Never call system APIs directly.
- **Manifest type threading** — The manifest type flows through generics so that volume names, image IDs, and dependency IDs are type-constrained.
- **Re-export from the SDK** — If you add a new export to `@start9labs/start-core`, also re-export it from the SDK's `lib/index.ts` (or expose it through `StartSdk.build()`).

### Adding OS Bindings

Types in `shared-libs/ts-modules/start-core/lib/osBindings/` mirror Rust types from the monorepo's `shared-libs/crates/start-core` (the `start_core` lib). When those Rust types change, the corresponding TypeScript bindings need regenerating. These are re-exported through `shared-libs/ts-modules/start-core/lib/osBindings/index.ts`.

### Writing Tests

- Place test files next to the code they test, or in the `test/` directory
- Use the `.test.ts` extension
- Tests run in Node.js with ts-jest — no browser environment
