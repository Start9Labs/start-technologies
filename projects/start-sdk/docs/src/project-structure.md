# Project Structure

Every StartOS service package follows a standard directory layout. This page documents the purpose of each file and directory in the project.

## Root Directory Layout

A StartOS package follows this organizational pattern:

```
my-service-startos/
├── .github/
│   └── workflows/
│       ├── build.yml          # CI build on PR
│       ├── tagAndRelease.yml  # Version check, tag, and release on merge
│       └── release.yml        # Release on manual tag push
├── assets/                 # Supplementary files (required, can be empty)
│   └── ABOUT.md
├── startos/                # Primary development directory
│   ├── actions/            # User-facing action scripts
│   ├── fileModels/         # Type-safe config file representations
│   ├── i18n/               # Internationalization
│   │   ├── index.ts        # setupI18n() call (boilerplate)
│   │   └── dictionaries/
│   │       ├── default.ts  # English strings keyed by index
│   │       └── translations.ts  # Translations for other locales
│   ├── init/               # Container initialization logic
│   ├── manifest/           # Static service metadata
│   │   ├── index.ts        # setupManifest() call
│   │   └── i18n.ts         # Static translations: manifest descriptions
│   ├── backups.ts          # Backup volumes and exclusions
│   ├── dependencies.ts     # Service dependencies
│   ├── index.ts            # Exports (boilerplate)
│   ├── interfaces.ts       # Network interface definitions (optional - not in barebones scaffold)
│   ├── main.ts             # Daemon runtime and health checks
│   ├── sdk.ts              # SDK initialization (boilerplate)
│   ├── utils.ts            # Package-specific utilities (empty in barebones scaffold)
│   └── versions/           # Version management and migrations
├── .gitignore
├── AGENTS.md               # Agent context: repo identity + how to work in this repo
├── CLAUDE.md               # One-line `@AGENTS.md` import for Claude Code
├── Dockerfile              # Optional - for custom images
├── icon.svg                # Service icon (max 40 KiB)
├── instructions.md         # User-facing instructions packed into the .s9pk (see Writing Instructions)
├── LICENSE                 # Package license (symlink to upstream)
├── Makefile                # Project config (includes the SDK's s9pk.mk from node_modules)
├── package.json
├── package-lock.json
├── README.md               # Service documentation (see Writing READMEs)
├── TODO.md                 # Pending work on the package
├── tsconfig.json
├── UPDATING.md             # Per-package upstream-version tracking
└── upstream-project/       # Git submodule (optional)
```

## Core Files

### Boilerplate Files

These files typically require minimal modification:

- `.gitignore`
- `Makefile` - Includes the SDK's `s9pk.mk` from `node_modules` (see [Makefile](./makefile.md))
- `package.json` / `package-lock.json`
- `tsconfig.json`

### .github/workflows/

Every package should include three GitHub Actions workflows that delegate to the reusable CI workflows in this monorepo (`.github/workflows/`, migrated from the old `shared-workflows` repo). The CI pipeline has two automatic stages, plus an optional manual path:

```
PR opened/updated ──> Build
PR merged to master ──> Version check ──> Tag ──> Build ──> Release ──> Publish
Manual tag push ──> Build ──> Release ──> Publish (bypasses version check)
```

Tags created by GitHub Actions (via `GITHUB_TOKEN`) do not trigger other workflows. The tag pushed by **tagAndRelease** will _not_ trigger the standalone **release.yml** — instead, tagAndRelease calls release directly as a reusable workflow. The standalone **release.yml** only runs when a tag is pushed manually.

**build.yml** -- builds the `.s9pk` on PR to verify it compiles:

```yaml
name: Build

on:
  workflow_dispatch:
  pull_request:
    branches: ['master']
    paths-ignore: ['*.md']

concurrency:
  group: ${{ github.workflow }}-${{ github.head_ref || github.ref }}
  cancel-in-progress: true

jobs:
  build:
    if: github.event.pull_request.draft == false
    uses: Start9Labs/start-technologies/.github/workflows/build.yml@master
    secrets:
      DEV_KEY: ${{ secrets.DEV_KEY }}
```

**tagAndRelease.yml** -- on merge to master, checks the version against the production registry. If the version already exists, the workflow exits gracefully without building. Otherwise, creates a release tag, then builds and publishes to the test registry. If a new commit arrives while a previous run is still in progress, the old run is cancelled:

```yaml
name: Tag and Release

on:
  push:
    branches: ['master']
    paths-ignore: ['*.md']

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  tag-and-release:
    uses: Start9Labs/start-technologies/.github/workflows/tagAndRelease.yml@master
    with:
      REFERENCE_REGISTRY: ${{ vars.REFERENCE_REGISTRY }}
      RELEASE_REGISTRY: ${{ vars.RELEASE_REGISTRY }}
      S3_S9PKS_BASE_URL: ${{ vars.S3_S9PKS_BASE_URL }}
    secrets:
      DEV_KEY: ${{ secrets.DEV_KEY }}
      S3_ACCESS_KEY: ${{ secrets.S3_ACCESS_KEY }}
      S3_SECRET_KEY: ${{ secrets.S3_SECRET_KEY }}
    permissions:
      contents: write
```

**release.yml** -- publishes on manual tag push, for re-releases or testing. This workflow only triggers on manually pushed tags — tags created by tagAndRelease (via `GITHUB_TOKEN`) do not trigger it:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*.*'

jobs:
  release:
    uses: Start9Labs/start-technologies/.github/workflows/release.yml@master
    with:
      RELEASE_REGISTRY: ${{ vars.RELEASE_REGISTRY }}
      S3_S9PKS_BASE_URL: ${{ vars.S3_S9PKS_BASE_URL }}
    secrets:
      DEV_KEY: ${{ secrets.DEV_KEY }}
      S3_ACCESS_KEY: ${{ secrets.S3_ACCESS_KEY }}
      S3_SECRET_KEY: ${{ secrets.S3_SECRET_KEY }}
    permissions:
      contents: write
```

### AGENTS.md and CLAUDE.md

`AGENTS.md` is the package's agent-context file. Generic packaging knowledge — SDK patterns, the disciplines on the [Development Workflow](./workflow.md) page, the rules throughout this guide — lives in one canonical place: the packaging guide, **not** copied into each package repo where 40+ duplicates would drift out of sync. `AGENTS.md` carries only what's specific to _this_ repo.

Keep it short and repo-specific: state that this is a StartOS service package, point at the repo's `TODO.md` as the worklist, give the doc-sync rule (keep `README.md` and `instructions.md` in step with every change), and capture any package-specific gotchas — in short, how to work in _this_ repo. Do **not** restate generic guide content or turn it into a web-fetch driver (don't instruct the agent to pull guide pages over the web up front). Developers work with the guide checked out locally alongside the package (see [Environment Setup](./environment-setup.md)); the local-first navigation — read `start-technologies/projects/start-sdk/docs/src/` directly, fall back to <https://docs.start9.com/packaging> only when no local copy exists — is set up once by the workspace-level `CLAUDE.md`, not repeated per repo.

`CLAUDE.md` is a one-line import of that same file:

```
@AGENTS.md
```

Claude Code auto-loads `CLAUDE.md` when it opens the repo, and the `@AGENTS.md` import pulls in the pointer so the same entry point covers both Claude and any other agent that reads `AGENTS.md` by convention. Don't duplicate anything into `CLAUDE.md`; keep the content in `AGENTS.md` and let the import do the work.

### Dockerfile (optional)

It is recommended to pull an existing Docker image as shown in the [Quick Start](./quick-start.md). If necessary, you can define a custom image using a Dockerfile in the project root.

### icon.svg

The service's visual identifier. Maximum size is 40 KiB. Accepts `.svg`, `.png`, `.jpg`, and `.webp` formats.

### instructions.md

User-facing instructions packed into the `.s9pk` and rendered on the **Instructions** tab in StartOS after install. Required at the package root — the build fails if missing. See [Writing Instructions](./writing-instructions.md) for what belongs in this file (and what does not).

### LICENSE

The package's software license, which should always match the upstream service's license. If your package contains multiple upstream services with different licenses, select the more restrictive license.

If you have a git submodule, symlink to its license:

```bash
ln -sf upstream-project/LICENSE LICENSE
```

If you are pulling a pre-built Docker image (no submodule), copy the license text directly from the upstream repository.

### README.md

Service documentation following the structure described in [Writing READMEs](./writing-readmes.md). Every README should document how the StartOS package differs from the upstream service.

### TODO.md

A running list of pending work on this package. Add items when you defer work; remove them when complete. An empty `TODO.md` (just the `# TODO` heading) is fine — keep the file present so contributors know where to record items.

### UPDATING.md

Per-package upstream-version tracking. Each package wraps one or more upstream sources (a Docker image, a git submodule, a Start9-built image), and the exact registry, tag format, and pinned field differs. `UPDATING.md` captures that detail so a bump can be applied without rediscovering it each time.

It has two sections:

- **Determining the upstream version** — for each upstream this package pulls, the canonical place to find the latest version (e.g. `gh release view -R <org>/<repo> --json tagName -q .tagName`, a Docker Hub tags listing, etc.) and the manifest field where the current pin lives (typically `images.<name>.source.dockerTag` in `startos/manifest/index.ts`).
- **Applying the bump** — the exact file and field to edit, including any tag-format quirks (e.g. drop the leading `v`, append `-alpine`, keep the major version aligned with a sibling image).

Packages with multiple upstream sources (e.g. a service plus its database sidecar) get one subsection per source under each heading. The cross-cutting rule about renaming the file in `startos/versions/` versus creating a new one lives in [Versions](./versions.md).

## assets/

Stores supplementary files and scripts needed by the service, such as configuration generators or entrypoint scripts. **Required** -- the `assets/` directory must exist and contain at least one file (e.g. `ABOUT.md`) for git to track it and for the build to succeed.

## startos/

The `startos/` directory is where you take advantage of the StartOS SDK and APIs. This is the primary development directory containing all SDK integration files and package logic.

### Core TypeScript Modules

| File              | Purpose                                                    |
| ----------------- | ---------------------------------------------------------- |
| `main.ts`         | Daemon runtime configuration and health checks             |
| `interfaces.ts`   | Network interface definitions and port bindings (optional) |
| `backups.ts`      | Backup volumes and exclusion patterns                      |
| `dependencies.ts` | Service dependencies and version requirements              |
| `sdk.ts`          | SDK initialization (boilerplate)                           |
| `utils.ts`        | Package-specific constants and helper functions            |
| `index.ts`        | Module exports (boilerplate)                               |

#### backups.ts

`setupBackups()` is where you define what volumes to back up as well as what directories or files to _exclude_ from backups.

#### dependencies.ts

`setupDependencies()` is where you define any dependencies of this package, including their versions, whether or not they need to be running or simply installed, and which health checks, if any, need to be passing for this package to be satisfied.

#### index.ts

This file is plumbing, used for exporting package functions to StartOS.

#### interfaces.ts (optional)

`setupInterfaces()` is where you define the service interfaces and determine how they are exposed. This function executes on service install, update, and config save. It takes the user's config input as an argument, which will be `null` for install and update.

The barebones scaffold ships no `interfaces.ts` — many services (background workers, sidecars) expose nothing on the network. When a service does, add this file and wire its `setInterfaces` into `init/index.ts` (conventionally before `setDependencies`).

#### main.ts

`setupMain()` is where you define the daemons that compose your service's runtime. It runs each time the service is started. Daemon comes with built-in health checks that can optionally be displayed to the user. You can also use `setupMain()` to define additional health checks, such as tracking and displaying a sync percentage.

#### manifest/

The manifest directory defines static metadata about the service, such as ID, name, description, release notes, helpful links, volumes, images, hardware requirements, and dependencies. See [Manifest](./manifest.md) for details.

#### sdk.ts

This file is plumbing, used to imbue the generic Start SDK with package-specific type information defined in `manifest.ts` and `store.ts`. The exported SDK is what should be used throughout the `startos/` directory. It is a custom SDK just for this package.

#### utils.ts

This file is for defining constants and functions specific to your package that are used throughout the code base. Many packages will not make use of this file.

### Subdirectories

| Directory     | Purpose                                                               |
| ------------- | --------------------------------------------------------------------- |
| `actions/`    | Custom user-facing scripts displayed as buttons in the UI             |
| `fileModels/` | Type-safe representations of config files (.json, .yaml, .toml, etc.) |
| `i18n/`       | Internationalization: default dictionary and translated strings       |
| `init/`       | Container initialization logic (install, update, restart)             |
| `manifest/`   | Service metadata (ID, name, description, images) with i18n            |
| `versions/`   | Version management and migration logic                                |

### actions/

```
actions/
├── index.ts
├── action1.ts
└── action2.ts
```

In the `actions/` directory, you define custom actions for your package.

Actions are predefined scripts that display as buttons to the user. They accept arbitrary input and return structured data that can be optionally displayed masked or as QR codes. For example, a `config.ts` action might present a validated form that represents an underlying config file of the service, allowing users to configure the service without needing SSH or the command line. A `resetPassword` action could use the upstream service's CLI to generate a new password for the primary admin, then display it to the user.

Each action receives its own file and is also passed into `Actions.of()` in `actions/index.ts`.

### fileModels/ (optional)

```
fileModels/
├── store.json.ts
└── config.json.ts
```

In the `fileModels/` directory, you can create separate `.ts` files from which you export a file model for each file from the file system you want to represent. Supported file formats are `.yaml`, `.toml`, `.json`, `.env`, `.ini`, `.txt`. For alternative file formats, you can use the `raw` method and provide custom serialization and parser functions.

These `.ts` files afford a convenient and type-safe way for your package to read, write, monitor, and react to files on the file system.

It is common for packages to have a `store.json.ts` file model as a convenient place to persist arbitrary data that are needed by the package but _not_ persisted by the upstream service. For example, you might use `store.json` to persist startup flags or login credentials.

### init/

```
init/
├── index.ts
├── taskCreateAdmin.ts
└── seedDatabase.ts
```

In the `init/` directory, you define the container initialization sequence for your package as well as optional custom init functions. Name each init file specifically for what it does (e.g., `taskCreateAdmin.ts`, `seedDatabase.ts`) rather than using a generic name like `initializeService.ts`.

Container initialization takes place under the following circumstances:

1. Package install (including fresh install, update, downgrade, and restore)
2. _Server_ (not service) restart
3. "Container Rebuild" (a built-in Action that must be manually triggered by the user)

> [!NOTE]
> Starting or restarting a service _does not_ trigger container initialization. Even if a service is stopped, the container still exists with event listeners still active.

#### init/index.ts

`setupInit()` is where you define the specific order in which functions will be executed when your container initializes.

- `restoreInit` and `versionGraph` must remain first and second. Do not move them.
- `setInterfaces`, `setDependencies`, `actions` are recommended to remain in this order, but could be rearranged if necessary.
- Any custom init functions can be appended to the list of built-in functions, or even inserted between them. Most custom init functions are simply appended to the list.

It is possible to limit the execution of custom init functions to specific _kinds_ of initialization. For example, if you only wanted to run a particular init function on fresh install and ignore it for updates and restores, `setupOnInit()` provides a `kind` variable (one of `install`, `update`, `restore`) that you can use for conditional logic. `kind` can also be null, which means the container is being initialized due to a server restart or manual container rebuild, rather than installation.

### versions/

```
versions/
├── index.ts
├── current.ts        # The latest version — always this filename
└── v1.0.2_0.ts       # A historical version kept for its migration
```

In the `versions/` directory, you manage package versions and define migration logic. The latest version always lives in `current.ts`; historical versions kept for migrations sit beside it under version-named files. The `index.ts` file uses `VersionGraph.of()` to index the current version and any previous versions of your package. Each version file uses `VersionInfo.of()` to provide the version number, release notes, and any migrations that should run.

Migration `up` and `down` functions run once, before anything else, _upon updating or downgrading to that version only_.

See [Versions](./versions.md) for full details.

> [!WARNING]
> Migrations are only for migrating data that is _not_ migrated by the upstream service itself.
