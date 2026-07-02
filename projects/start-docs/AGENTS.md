# AGENTS.md

Operating instructions for AI developers working on the docs-site project (the `projects/start-docs/` project in the `start-os` monorepo). `CLAUDE.md` just imports this file. See `ARCHITECTURE.md` for how the build works and `CONTRIBUTING.md` for the human workflow.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## What this is

This project owns the **site build infra** (`build.sh`, `serve.sh`, `versions.conf`, `theme/`, `scripts/`), the **landing page** (`landing/`), and the **Bitcoin Guides** book (`bitcoin-guides/`).

## Layout

The StartOS, StartTunnel, Packaging, and StartWRT books are NOT here — they moved into their product dirs:

- StartOS → `../start-os/docs/`
- StartTunnel → `../start-tunnel/docs/`
- Packaging (book name `packaging`) → `../start-sdk/docs/`
- StartWRT → `../start-wrt/docs/`

`build.sh`'s `book_dir()` maps each book name to its source dir. If you're editing content for one of those products, edit it in the product dir, not here — but you can build/preview the whole site from here.

## Build & test (run from `projects/start-docs/`)

- `./build.sh` — builds every book in `versions.conf` into the gitignored `docs/` output dir. Run this to verify your change compiles (mdBook fails on broken intra-book links). This is the primary check.
- `./serve.sh` — build + serve at http://localhost:3000.
- Single book live-reload: `cd <book-src-dir> && mdbook serve -p 3001` (e.g. `cd bitcoin-guides`, or `cd ../start-os/docs`).
- `cd scripts && npm run generate-llms-txt` — regenerate `llms.txt` / `llms-full.txt` (uses `tsx`).
- Tooling: mdBook **v0.5.2**, mdbook-tabs **0.3.4** (match CI). Node v22+ for scripts.

## Gotchas

- Always re-read a file before subsequent edits — a linter/formatter may auto-modify files after changes.
- Never use custom admonition titles. `> [!WARNING] Custom Title` is broken in mdBook; use plain `> [!WARNING]` and put context in the body.
- Avoid nested tabs. Use separate sections with single-level tabs. OS pickers use `global="platform"` with the canonical label set (`Mac`, `Windows`, `Linux`, `iOS`, `Android / Graphene`) — see CONTRIBUTING for the tab rules.
- Cross-book links must use absolute paths (`/start-tunnel/devices.html`), not relative paths — mdBook only validates intra-book links.
- All pages are flat in each book's `src/` — no subdirectory nesting. Sidebar sections use `# Part Title` in `SUMMARY.md`.
- Every page should have introductory prose between the H1 and the first H2. It's auto-extracted for `llms.txt`.
- When creating a new page, add it to the book's `src/SUMMARY.md` or it won't appear in the sidebar or build.
- `theme/` here is the single source of truth; books symlink to it. Edit theme assets here, not in a book's symlinked copy.

## Adding or moving a book

1. Add `book-name=version` to `versions.conf` (build, deploy, and nginx routing all derive from it — no other config to touch).
2. If the book lives outside this project, add a `book_dir()` case in `build.sh` pointing at its source dir. Books with no mapping default to `<book-name>/` (relative to this project).

## Deployment

GitHub Actions `.github/workflows/docs-deploy.yml` (at the monorepo root) builds and rsyncs to the VPS on push to `master` touching `projects/start-docs/**`, `projects/start-os/docs/**`, `projects/start-tunnel/docs/**`, `projects/start-sdk/docs/**`, or `projects/start-wrt/docs/**`. It regenerates nginx routing from `versions.conf`. Don't hardcode book names in nginx — the generated `book_versions.conf` handles that.
