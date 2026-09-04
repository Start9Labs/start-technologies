# AGENTS.md — shared-libs/ts-modules

Agent/dev instructions for `shared-libs/ts-modules` — the directory of shared TypeScript modules: the two Angular libs `@start9labs/shared`, `@start9labs/marketplace`, and the non-Angular `@start9labs/start-core` (`start-core/` — the SDK's core types/ABI/effects/OS bindings, consumed by web and bundled into the SDK; it has its own `Makefile`/`package.json` and builds outside the Angular workspace). The Angular workspace root config (`angular.json`, `package.json`, `tsconfig.json`) lives at the repo root. `CLAUDE.md` is a one-line `@AGENTS.md` import. See `ARCHITECTURE.md` for structure, `CONTRIBUTING.md` for full setup.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- **The workspace root is the repo root.** `angular.json`, `package.json`, `tsconfig.json` all live at the repo root. The Angular libs `shared/` and `marketplace/` live here, alongside the non-Angular `start-core/` (`@start9labs/start-core`), which has its own `Makefile`/`package.json` and is built separately (not part of the Angular workspace).
- **Apps live elsewhere.** `ui` → `../../projects/start-os/web/ui`, `setup-wizard` → `../../projects/start-os/web/setup-wizard`, `start-tunnel` → `../../projects/start-tunnel/web`, `start-wrt` → `../../projects/start-wrt/web`, `brochure-marketplace` → `../../projects/brochure-marketplace`. Editing app code means editing those dirs even though `ng`/`tsc` are run from the repo root.
- i18n dictionaries: `shared/src/i18n/dictionaries/`.

## Build & test (run from the repo root)

```sh
npm ci
npm run build:deps           # MUST run first after install — builds the file: deps (@start9labs/start-core, patch-db client)
npm run check                # type-check all projects; or check:shared / check:ui / etc. for one
make -C shared-libs/ts-modules/start-core test   # jest; start-core only, see Gotchas
make web-format              # prettier; make web-format-check for CI
npm run start:ui             # mock dev server (needs config.json — cp shared-libs/ts-modules/config-sample.json config.json)
npm run build:ui             # prod build of a single app
```

## Gotchas

- `@start9labs/start-sdk` and `patch-db-client` are `file:` deps built by `build:deps`; a fresh checkout won't type-check until you run it.
- `start-core` has a jest suite in `start-core/lib/test/`, run by `make -C shared-libs/ts-modules/start-core test` and reached by the root `make test` in CI. The Angular libs have no test runner — for them `npm run check` (tsc, strict + strictTemplates) plus a successful `build:*` is the verification bar.
- `shared-libs/crates/patch-db` is a first-party crate; `build:deps` runs `npm ci && npm run build` inside it.

- **Frontend house style lives in the `start9-frontend` skill** at the repo root (`.claude/skills/start9-frontend/`): `SKILL.md` (doctrine, surprise index, review checklist) plus per-topic references (components, styling, forms, overlays, state, i18n, the antipattern catalog, a verified Taiga 5 API reference). Claude Code loads it automatically; other agents and humans read `SKILL.md` first, then the reference for the topic at hand. Where older docs or existing code disagree with the skill, the skill wins — and never guess a Taiga API: verify via the skill's `references/taiga.md`, the `taiga-ui-mcp` MCP server, or `https://taiga-ui.dev/llms-full.txt`. **Keep the skill current**: it is the fleet-wide frontend source of truth — `start9-store`, `ops-server`, and `support-server` reach this exact copy through committed symlinks, and stack-version facts live only in its fleet table — so when frontend conventions, versions, or idioms change, update the skill in the same change.
- **`brochure-marketplace` (`../../projects/brochure-marketplace`) is a public website, not an embedded OS app.** It's the marketplace front at marketplace.start9.com and **auto-deploys on merge to `master`** (`.github/workflows/deploy-brochure.yml`) — `ui`, `setup-wizard`, and `start-tunnel` ship inside the OS image instead, and `start-wrt` ships embedded in the `startwrt` binary. brochure consumes the same source `shared`/`marketplace` libs as the other apps.
