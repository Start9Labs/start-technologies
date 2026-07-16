# AGENTS.md — start-sdk

The TypeScript SDK (`@start9labs/start-sdk`) for building StartOS service packages. Lives at `projects/start-sdk/` inside the start-technologies monorepo. One npm package plus the packaging build wrapper and the packaging mdbook. `CLAUDE.md` is a one-line `@AGENTS.md` import.

This file is **both** the contribution guide and the agent/dev operating rules for this scope — how to set up, build, test, and release the SDK, plus the gotchas. It has no `CONTRIBUTING.md`: this scope's was folded in here (the rest of the monorepo still carries one per scope; that migration is in progress). See [`ARCHITECTURE.md`](ARCHITECTURE.md) for how the SDK is structured — modules, data flow, the build pipeline, and the design patterns (builder chains, effects-as-capabilities, manifest type threading). If you are building a service package _using_ the SDK rather than developing the SDK itself, you want the [packaging docs](https://docs.start9.com/packaging).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up — commit/PR conventions live in the root [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Prerequisites

Node.js v22+ (nvm recommended), npm, and GNU Make.

## Layout

- `lib/` — `@start9labs/start-sdk`: developer-facing facade (`StartSdk`), daemons, health checks, backups, file helpers, subcontainers, i18n, triggers. Imports core types, OS bindings, ABI, `Effects`, ExVer parser, actions/input builders, interfaces, dependencies, s9pk reader from `@start9labs/start-core` (`shared-libs/ts-modules/start-core/`).
- `dist/` — build output (generated; what publishes to npm). It bundles `@start9labs/start-core` (via npm `bundleDependencies`) so it stays self-contained. **Container-runtime consumes the built `dist/`, not the source.**
- `Makefile` — build orchestration for the SDK itself.
- `s9pk.mk`, `tsconfig.base.json` — build plumbing shipped _inside_ the published package for service packages to `include`/`extends`. Marked DO NOT EDIT in the consuming-package contract; edits here change the contract for every package.
- `docs/` — the "Service Packaging" mdbook (`book.toml`), published at docs.start9.com/packaging. Also carries `package-template/` and the workspace agent context. See [Docs](#the-docs-mdbook).
- `CHANGELOG.md` — Keep a Changelog style, headings `## <sdk-version> — StartOS <os-version> (<date>)`.

## Build & test (run from `projects/start-sdk/`)

| Command                       | What                                                                                                        |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `make node_modules`           | `npm ci`                                                                                                    |
| `make bundle`                 | full build: build `@start9labs/start-core` (prerequisite), compile SDK → `dist/`, then `test` + `check-fmt` |
| `make dist`                   | compile SDK (depends on start-core)                                                                         |
| `make test`                   | jest                                                                                                        |
| `make check`                  | `tsc --noEmit`                                                                                              |
| `make fmt` / `make check-fmt` | Prettier write / check on all `.ts`                                                                         |
| `make link`                   | build + `npm link` from `dist/` for local package testing                                                   |
| `make clean`                  | remove `dist/`, `node_modules`, generated test output                                                       |
| `make publish`                | the raw npm step only — **not** how you cut a release (see Gotchas)                                         |

Tests are jest + ts-jest, Node only (no browser). Test files use `.test.ts` and are excluded from compilation via `tsconfig.json`. Run one with `npx jest --testPathPattern=host`. The bundled `@start9labs/start-core` has its own suite and build: `cd ../../shared-libs/ts-modules/start-core && make test` (or `make dist`). The ExVer parser is generated from that lib's `lib/exver/exver.pegjs` via Peggy (`make` runs this for you).

Both packages are strict TypeScript, ES2021 target, CommonJS output. [`ARCHITECTURE.md`](ARCHITECTURE.md#build-pipeline) covers what the build actually does.

## Testing SDK changes against a service package

No publish needed — build and link:

```bash
make link                        # from projects/start-sdk/
npm link @start9labs/start-sdk   # from your service package
```

This symlinks the built `dist/` into your global `node_modules`, so the package picks up local SDK changes.

## Cutting a release

The SDK is a first-class project of the monorepo-wide release tool, [`scripts/manage-release.sh`](../../scripts/manage-release.sh) (the `npm` kind). The version is read from `package.json`; the git tag / GitHub release is `start-sdk/v<version>`. Only `dist/` ships to npm (compiled JavaScript, declarations, bundled dependencies, package metadata).

1. Bump `package.json` and add the matching `CHANGELOG.md` entry (`pre-check` requires it), then land that on `master`.
2. Cut the release from the repo root (needs `gh` and an npm login with publish rights):

   ```bash
   ./scripts/manage-release.sh release start-sdk
   ```

   This runs pre-check → tag → create the GitHub release → `npm publish`. It prompts for your npm 2FA one-time password at publish time; `OTP=…` skips the prompt.

The step order is deliberate: everything idempotent runs _before_ the one irreversible step. See the Gotcha below for why that matters, and never publish with `make publish`.

### Backfilling a release

If a version reached npm without a tag and release, cut them after the fact with the individual subcommands. The commit to tag is the one the published tarball was built from, which npm records:

```bash
npm view @start9labs/start-sdk@<version> gitHead

VERSION=<version> COMMIT=<sha> ./scripts/manage-release.sh tag start-sdk
VERSION=<version> ./scripts/manage-release.sh create-gh-release start-sdk
```

If that commit never landed on `master` (e.g. the publish was cut from an unmerged branch), merge it first and tag the resulting `master` commit instead — verify its shipped subtree matches the published tarball rather than assuming it does.

## Gotchas

- **Releasing is `./scripts/manage-release.sh release start-sdk`, not `make publish`.** The pipeline is pre-check → tag → GitHub release → `npm publish`, in that order because npm publish is the one step that can never be redone. `make publish` is _only_ that last step: run it on its own and the version lands on npm with no git tag and no GitHub release, and the normal flow can't recover (pre-check then refuses the version, and npm won't republish it). 2.0.4 and 2.0.5 shipped this way and had to be backfilled. See [Cutting a release](#cutting-a-release) above, which also documents the backfill.
- **Bumping the version requires a CHANGELOG entry.** Freshly check what's shipped first (`git ls-remote --tags origin 'start-sdk/v*'`, or `npm view @start9labs/start-sdk versions`); the top `CHANGELOG.md` heading is the prospective next SDK version. If it has no matching `start-sdk/v<version>` tag it is unreleased — add your entry under it (`### Added/Changed/Fixed/Removed`), raising the number and `package.json` `version` only for a larger tier (see the next bullet, and the root [`AGENTS.md`](../../AGENTS.md) changelog rule). Reviews reject version bumps without a changelog entry.
- **Don't bump if the current latest hasn't published to npm.** Edit the unpublished version in place (promote patch→minor if the change warrants).
- **Consumers read the built output.** After editing the SDK or `@start9labs/start-core`, run `make bundle` before checking container-runtime.
- **SDK vs start-core:** types/ABI/OS-bindings/low-level → `@start9labs/start-core` (`shared-libs/ts-modules/start-core/`); developer-facing wrappers/runtime helpers → the SDK's `lib/`. A new start-core export must be re-exported from `lib/index.ts` or exposed via `StartSdk.build()`.
- **OS bindings** (`shared-libs/ts-modules/start-core/lib/osBindings/`) mirror Rust types in `shared-libs/crates/start-core`; regenerate/update them when the Rust side changes.
- **Editing `s9pk.mk` / `tsconfig.base.json` changes every package's build** — they ship in the published package. Treat as a public contract.
- Prettier config (single quotes, no semis, trailing commas, 2-space, `arrowParens: avoid`) lives in each sub-package's `package.json`.

## Docs

`README.md` (overview + quickstart), `ARCHITECTURE.md` (modules + data flow), `CHANGELOG.md`, and this file (contribute + operate). The packaging mdbook in `docs/` is the developer-facing reference — update it when you change the SDK's developer surface. Keep all of these current in the same change that alters structure, conventions, build, or surface.

### The `docs/` mdbook

Authoring conventions shared by every book in the monorepo — mdBook/mdbook-tabs versions, admonitions, tabs, `SUMMARY.md`, the shared `theme/` symlink, cross-book links — live in [`projects/start-docs/AGENTS.md`](../start-docs/AGENTS.md) and its `CONTRIBUTING.md`. Read those before editing pages. What is specific to _this_ book:

- **`docs/src/agent-context.md` ships to every packager.** `start-cli s9pk init-workspace` symlinks it in as each workspace's `AGENTS.md` (the `AGENTS_SYMLINK_TARGET` const in [`shared-libs/crates/start-core/src/s9pk/init.rs`](../../shared-libs/crates/start-core/src/s9pk/init.rs)), so an edit reaches every workspace on its next guide sync. It is an always-on context file first and a book page second: keep it a lean map that points at pages, never a place to inline detail. **Moving or renaming it breaks every existing workspace symlink** — update the const in the same change.
- **`docs/package-template/` is live code, not an illustration.** `s9pk init-package` copies it verbatim, interpolating `{{id}}` and `{{name}}` (escaped for TypeScript string literals in `.ts` files) and skipping `node_modules/`, `.git/`, and `javascript/`. Keep it buildable; a broken template breaks every new package. Its `.github/workflows/` are a hand-maintained mirror — see the repo-root `AGENTS.md` § Coupled changes.
- **Recipes name constructs; reference pages teach them.** Code examples belong on reference pages. A new `recipe-*.md` needs an entry in the intent table in `docs/src/recipes.md`, not just in `SUMMARY.md`.
- **Verify SDK claims against `lib/`, not against the prose.** This guide has shipped confidently-worded semantics that were wrong. Before documenting what a call does, read it.
