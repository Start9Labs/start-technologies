# AGENTS.md — shared-libs/ts-modules

Agent/dev instructions for `shared-libs/ts-modules` — the directory of shared TypeScript modules: the two Angular libs `@start9labs/shared`, `@start9labs/marketplace`, and the non-Angular `@start9labs/start-core` (`start-core/` — the SDK's core types/ABI/effects/OS bindings, consumed by web and bundled into the SDK; it has its own `Makefile`/`package.json` and builds outside the Angular workspace). The Angular workspace root config (`angular.json`, `package.json`, `tsconfig.json`) lives at the repo root. `CLAUDE.md` is a one-line `@AGENTS.md` import. See `ARCHITECTURE.md` for structure.

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

- `@start9labs/start-core` and `patch-db-client` are `file:` deps built by `build:deps`; a fresh checkout won't type-check until you run it.
- `start-core` has a jest suite in `start-core/lib/test/`, run by `make -C shared-libs/ts-modules/start-core test` and reached by the root `make test` in CI. The Angular libs have no test runner — for them `npm run check` (tsc, strict + strictTemplates) plus a successful `build:*` is the verification bar.
- `shared-libs/crates/patch-db` is a first-party crate; `build:deps` runs `npm ci && npm run build` inside its `client/` directory.
- **`brochure-marketplace` (`../../projects/brochure-marketplace`) is a public website, not an embedded OS app.** It's the marketplace front at marketplace.start9.com and **auto-deploys on merge to `master`** (`.github/workflows/deploy-brochure.yml`) — `ui` and `setup-wizard` ship inside the OS image; `start-tunnel` ships inside `tunnelbox`, and `start-wrt` ships embedded in the `startwrt` binary. brochure consumes the same source `shared`/`marketplace` libs as the other apps.

## Configure `config.json`

```sh
cp shared-libs/ts-modules/config-sample.json config.json
```

- By default, "useMocks" is set to `true`.
- Use "maskAs" to mock the host from which the web UI is served. Valid values are `tor`, `local`, `localhost`, `ipv4`, `ipv6`, and `clearnet`.
- Use "maskAsHttps" to mock the protocol over which the web UI is served. `true` means https; `false` means http.

## Development Server

You can develop using mocks (recommended to start) or against a live server. Code changes will live reload the browser.

### Using mocks

```sh
npm run start:setup
npm run start:ui
```

### Proxying to a live server

1. In `config.json`, set "useMocks" to `false`

2. Copy and configure the proxy config:

```sh
cp proxy.conf-sample.json proxy.conf.json
```

3. Replace every instance of `<CHANGEME>` with the hostname of your remote server

4. Start the proxy dev server:

```sh
npm run start:ui:proxy
```

## Translations

The shared dictionaries live in `shared/src/i18n/dictionaries/`. `en.ts` maps
English keys to numeric IDs; each other dictionary maps those IDs to translated
strings. Add a key at the next available ID and supply a real translation in
every dictionary. Run `npm run check:i18n`.

For a new language, add its dictionary, extend `loadDictionary` and the Taiga
language loader in `shared/src/i18n/i18n.providers.ts`, and update `languages`
and `LANGUAGE_TO_TUI` in `shared/src/i18n/i18n.service.ts`. Add its language-name
key and translations across the dictionaries. Coordinate matching backend
locale support and the product-local StartTunnel/StartWRT dictionaries.
