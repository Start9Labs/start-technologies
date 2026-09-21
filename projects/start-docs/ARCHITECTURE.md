# Architecture

How the documentation site is built, versioned, and deployed.

## Place in the monorepo

This is the `projects/start-docs/` project in the `start-os` monorepo. It owns the site build infra (`build.sh`, `serve.sh`, `versions.conf`), the shared mdBook `theme/`, the `landing/` page, and the Bitcoin Guides book — and it wires together the per-product books that live in their own product dirs (`../start-os/docs/`, `../start-tunnel/docs/`, `../start-sdk/docs/`, `../start-wrt/docs/`) into one deployed site. CI (`.github/workflows/docs-deploy.yml`) consumes its output to deploy `docs.start9.com`.

## Multi-Book Design

The site is composed of independent mdBook instances — one per product. Each book has its own `book.toml`, `src/SUMMARY.md`, and content tree. Books build into subdirectories of `docs/` (the build output, gitignored) and are deployed together under a shared domain.

The per-product books live **next to the code they document**, not inside this directory. This `docs/` project owns only the build infra, the shared theme, the landing page, and the Bitcoin Guides book.

```
start-os/ (monorepo root)
├── projects/start-os/docs/        ← StartOS book (book.toml, src/, theme -> ../../start-docs/theme)
├── projects/start-tunnel/docs/    ← StartTunnel book
├── projects/start-sdk/docs/       ← Service Packaging book (book name: "packaging")
├── projects/start-wrt/docs/       ← StartWRT book
└── projects/start-docs/           ← THIS project: site build + landing + bitcoin-guides
    ├── build.sh          ← builds all books into docs/ output
    ├── serve.sh          ← build + local dev server
    ├── versions.conf     ← book → version list (single source of truth)
    ├── theme/            ← shared theme (CSS, JS, favicon); books symlink here
    ├── landing/          ← static landing page at docs.start9.com/
    ├── scripts/          ← build-time tooling (llms.txt generator)
    └── bitcoin-guides/   ← Bitcoin Guides book
        ├── book.toml
        ├── theme -> ../theme
        └── src/ (SUMMARY.md, README.md, archival-vs-pruned.md, electrum-servers.md, ...)
```

This multi-book design was chosen over a single monolithic book because:

- Each product gets its own sidebar, search, and URL namespace
- Adding a new product means adding a book directory + one `versions.conf` line, not restructuring existing content
- Books share a flat page layout (all pages directly in `src/`) with sidebar section headers (`# Part Title` in `SUMMARY.md`)

## Book name → source dir mapping

`build.sh` decouples the book name (used in URLs and `versions.conf`) from its source directory:

```sh
book_dir() {
  case "$1" in
    start-os) echo "$ROOT/../start-os/docs" ;;
    start-tunnel) echo "$ROOT/../start-tunnel/docs" ;;
    packaging) echo "$ROOT/../start-sdk/docs" ;;
    start-wrt) echo "$ROOT/../start-wrt/docs" ;;
    *) echo "$ROOT/$1" ;;            # bitcoin-guides etc. live in docs/
  esac
}
```

So `packaging` is served from `projects/start-sdk/docs`, and any book not explicitly mapped is expected to live directly under this project (`projects/start-docs/`). To move or add a book, edit `book_dir()` and `versions.conf`.

## Shared Theme

`theme/` in this project is the single source of truth for styling. Each book symlinks to it (e.g. `bitcoin-guides/theme -> ../theme`, `start-os/docs/theme -> ../../start-docs/theme`). It includes:

- YouTube embed styling (`youtube.css` / `youtube.js`)
- mdbook-tabs CSS/JS (`tabs.css` / `tabs.js`)
- Theme toggle (`theme-toggle.js`) and home link (`home-link.js`)
- Favicon

Each book's `book.toml` references these under `additional-css` / `additional-js`.

## Versioning

Each book is versioned independently via `versions.conf` in this project:

```
start-os=0.4.0.x
start-tunnel=1.0.x
packaging=0.4.0.x
bitcoin-guides=1.0.x
```

`versions.conf` is the single source of truth — `build.sh` and the routing it writes into the tree (the redirect stubs and `404.html`) derive from it. Adding a book takes one line here (plus a `book_dir()` mapping if it lives outside this project).

Build output goes to `docs/<book>/<version>/` (e.g. `docs/start-os/0.4.0.x/`). `MDBOOK_OUTPUT__HTML__SITE_URL` is set per-book at build time so mdBook generates correct search indexes and canonical URLs for the versioned path.

## Build Pipeline

`build.sh`:

1. Wipes and recreates the `docs/` output dir
2. Iterates over `versions.conf`, resolves each book's source dir via `book_dir()`, and runs `mdbook build -d docs/<book>/<version>` with the versioned `SITE_URL`
3. Writes redirect stubs for the unversioned URLs: `docs/<book>/index.html` → `/<book>/<version>/` and `docs/<book>/<page>.html` → `/<book>/<version>/<page>.html` for every page, fragment preserved
4. Copies `landing/index.html` to `docs/index.html`, and writes `docs/404.html` from `landing/404.html` with the book list filled in. A static host serves that page for every path it has no file for, and its script applies the remaining routing rules: `/latest/*`, `/packaging-guide`, the one-off legacy redirects, extensionless page URLs, and unknown paths onto the `0.3.5.x` site
5. Runs the llms.txt generator (`scripts/generate-llms-txt.ts`, installing its deps on first use) to produce `llms.txt` (index) and `llms-full.txt` (full content) for LLM consumption, for the site and for each book

`docs/` is then the whole site: what the deploy publishes is what `./build.sh` leaves on disk.

## Deployment

Deployment is via GitHub Actions (`.github/workflows/docs-deploy.yml` at the monorepo root). It triggers on pushes to **`live-docs`** — not `master` — touching `projects/start-docs/**`, `projects/start-os/docs/**`, `projects/start-tunnel/docs/**`, `projects/start-sdk/docs/**`, or `projects/start-wrt/docs/**`, and its checkout is pinned to `live-docs` so a manual dispatch cannot publish another branch. Content reaches `live-docs` on a product tag (`docs-sync-on-tag.yml`) or by PR for fixes to already-published pages (`docs-backport.yml` then lands the same change on master). See [AGENTS.md](AGENTS.md#deployment). Steps:

1. Install mdBook (v0.5.2) and mdbook-tabs (0.3.4)
2. `./build.sh`
3. Publish `docs/` whole to the `docs.start9.com` folder on evelyn's NextExplorer through the `nextexplorer-publish` action (`.github/actions/`), which Start9 Pages serves as docs.start9.com

Pages serves files and nothing else, which is why every redirect the site needs is built into the tree (see the build pipeline above). The retired Sphinx site at `0.3.5.x` is seeded into that folder by hand and carried across each publish (`keep: 0.3.5.x`); nothing builds it.

`.github/workflows/deploy-docs-pages.yml` on `master` publishes on `live-docs`'s behalf until a product tag carries the publish step in `docs-deploy.yml` onto that branch — a PR to `live-docs` may not carry workflow files — and stands down by itself once it has; it can be deleted after that.

## Scripts

| Script                         | Purpose                                                                            |
| ------------------------------ | ---------------------------------------------------------------------------------- |
| `scripts/generate-llms-txt.ts` | Produces `llms.txt` (index) and `llms-full.txt` (full content) for LLM consumption |

Run via `cd scripts && npm run generate-llms-txt` (uses `tsx`).

## Cross-Book Links

mdBook validates links only within a single book. Links between books use unversioned absolute paths (`/start-tunnel/devices.html`) — the stub `build.sh` writes at that path sends the browser to the current version. They are not validated at build time, so keep them few and correct.

## Further reading

- [README.md](README.md) — what this project is and where the books live
- [CONTRIBUTING.md](CONTRIBUTING.md) — local setup and how to submit changes
- [AGENTS.md](AGENTS.md) — operating rules for AI developers (`CLAUDE.md` is a one-line `@AGENTS.md` import)
