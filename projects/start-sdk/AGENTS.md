# AGENTS.md — start-sdk

The TypeScript SDK (`@start9labs/start-sdk`) for building StartOS service packages. Lives at `projects/start-sdk/` inside the start-technologies monorepo. One npm package plus the packaging build wrapper and the packaging mdbook. `CLAUDE.md` is a one-line `@AGENTS.md` import. See `ARCHITECTURE.md` and `CONTRIBUTING.md` for structure and contribution details.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

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
| `make publish`                | build, then `npm publish` from `dist/` (`OTP=…` for 2FA, else prompts)                                      |

Tests are jest + ts-jest, Node only (no browser). Test files use `.test.ts`. The ExVer parser is generated from `shared-libs/ts-modules/start-core/lib/exver/exver.pegjs` via Peggy (`make` runs this for you).

## Gotchas

- **Bumping the version requires a CHANGELOG entry.** Edit `package.json` `version`, then add the heading + `### Added/Changed/Fixed/Removed` sections at the top of `CHANGELOG.md`. Reviews reject version bumps without it.
- **Don't bump if the current latest hasn't published to npm.** Edit the unpublished version in place (promote patch→minor if the change warrants).
- **Consumers read the built output.** After editing the SDK or `@start9labs/start-core`, run `make bundle` before checking container-runtime.
- **SDK vs start-core:** types/ABI/OS-bindings/low-level → `@start9labs/start-core` (`shared-libs/ts-modules/start-core/`); developer-facing wrappers/runtime helpers → the SDK's `lib/`. A new start-core export must be re-exported from `lib/index.ts` or exposed via `StartSdk.build()`.
- **OS bindings** (`shared-libs/ts-modules/start-core/lib/osBindings/`) mirror Rust types in `shared-libs/crates/start-core`; regenerate/update them when the Rust side changes.
- **Editing `s9pk.mk` / `tsconfig.base.json` changes every package's build** — they ship in the published package. Treat as a public contract.
- Prettier config (single quotes, no semis, trailing commas, 2-space, `arrowParens: avoid`) lives in each sub-package's `package.json`.

## Docs

`README.md` (overview + quickstart), `ARCHITECTURE.md` (modules + data flow), `CONTRIBUTING.md` (build/test/contribute), `CHANGELOG.md`, this file. The packaging mdbook in `docs/` is the developer-facing reference — update it when you change the SDK's developer surface. Keep all of these current in the same change that alters structure, conventions, build, or surface.

### The `docs/` mdbook

Authoring conventions shared by every book in the monorepo — mdBook/mdbook-tabs versions, admonitions, tabs, `SUMMARY.md`, the shared `theme/` symlink, cross-book links — live in [`projects/start-docs/AGENTS.md`](../start-docs/AGENTS.md) and its `CONTRIBUTING.md`. Read those before editing pages. What is specific to _this_ book:

- **`docs/src/agent-context.md` ships to every packager.** `start-cli s9pk init-workspace` symlinks it in as each workspace's `AGENTS.md` (the `AGENTS_SYMLINK_TARGET` const in [`shared-libs/crates/start-core/src/s9pk/init.rs`](../../shared-libs/crates/start-core/src/s9pk/init.rs)), so an edit reaches every workspace on its next guide sync. It is an always-on context file first and a book page second: keep it a lean map that points at pages, never a place to inline detail. **Moving or renaming it breaks every existing workspace symlink** — update the const in the same change.
- **`docs/package-template/` is live code, not an illustration.** `s9pk init-package` copies it verbatim, interpolating `{{id}}` and `{{name}}` (escaped for TypeScript string literals in `.ts` files) and skipping `node_modules/`, `.git/`, and `javascript/`. Keep it buildable; a broken template breaks every new package. Its `.github/workflows/` are a hand-maintained mirror — see the repo-root `AGENTS.md` § Coupled changes.
- **Recipes name constructs; reference pages teach them.** Code examples belong on reference pages. A new `recipe-*.md` needs an entry in the intent table in `docs/src/recipes.md`, not just in `SUMMARY.md`.
- **Verify SDK claims against `lib/`, not against the prose.** This guide has shipped confidently-worded semantics that were wrong. Before documenting what a call does, read it.
