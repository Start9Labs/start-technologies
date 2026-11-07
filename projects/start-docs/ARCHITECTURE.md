# Architecture

How the documentation site is built, versioned, and deployed.

## Place in the monorepo

This is the `projects/start-docs/` project in the `start-technologies` monorepo. It owns the site build infra (`build.sh`, `serve.sh`, `versions.conf`), the shared mdBook `theme/`, the `landing/` page, and the Bitcoin Guides and Start9 Support books — and it wires together the per-product books that live in their own product dirs (`../start-os/docs/`, `../start-tunnel/docs/`, `../start-sdk/docs/`, `../start-wrt/docs/`) into one deployed site. CI (`.github/workflows/docs-deploy.yml`) consumes its output to deploy `docs.start9.com`.

## Multi-Book Design

The site is composed of independent mdBook instances — one per product. Each book has its own `book.toml`, `src/SUMMARY.md`, and content tree. Books build into subdirectories of `docs/` (the build output, gitignored) and are deployed together under a shared domain.

The per-product books live **next to the code they document**, not inside this directory. This `docs/` project owns only the build infra, the shared theme, the landing page, and the Bitcoin Guides and Start9 Support books.

```
start-technologies/ (monorepo root)
├── projects/start-os/docs/        ← StartOS book (book.toml, src/, theme -> ../../start-docs/theme)
├── projects/start-tunnel/docs/    ← StartTunnel book
├── projects/start-sdk/docs/       ← Service Packaging book (book name: "packaging")
├── projects/start-wrt/docs/       ← StartWRT book
└── projects/start-docs/           ← THIS project: site build + landing + bitcoin-guides + support
    ├── build.sh          ← builds all books into docs/ output
    ├── serve.sh          ← build + local dev server
    ├── versions.conf     ← current and archived book entries consumed by build.sh
    ├── theme/            ← shared theme (CSS, JS, favicon); books symlink here
    ├── landing/          ← landing page, archives page and 404 router
    ├── scripts/          ← build-time tooling (index check, llms.txt generator)
    ├── bitcoin-guides/   ← Bitcoin Guides book
    │   ├── book.toml
    │   ├── theme -> ../theme
    │   └── src/ (SUMMARY.md, README.md, archival-vs-pruned.md, electrum-servers.md, ...)
    └── support/          ← Start9 Support book (support.start9.com's user guide)
```

This multi-book design was chosen over a single monolithic book because:

- Each product gets its own sidebar, search, and URL namespace
- Adding a product does not require restructuring existing book content
- Books share a flat page layout (all pages directly in `src/`) with sidebar section headers (`# Part Title` in `SUMMARY.md`)

## Book name → source dir mapping

`build.sh` decouples the book name (used in URLs and `versions.conf`) from its source directory, given relative to the repo root so the same path resolves in a release tag's tree:

```sh
book_path() {
  case "$1" in
    start-os) echo projects/start-os/docs ;;
    start-tunnel) echo projects/start-tunnel/docs ;;
    packaging) echo projects/start-sdk/docs ;;
    start-wrt) echo projects/start-wrt/docs ;;
    *) echo "projects/start-docs/$1" ;;   # bitcoin-guides, support
  esac
}
```

So `packaging` is served from `projects/start-sdk/docs`, and any book not explicitly mapped is expected to live directly under this project. To move or add a book, follow [Adding or moving a book](AGENTS.md#adding-or-moving-a-book), including the product mapping in `scripts/docs-versions.mjs`.

## Shared Theme

`theme/` in this project is the single source of truth for styling. Each book symlinks to it (e.g. `bitcoin-guides/theme -> ../theme`, `start-os/docs/theme -> ../../start-docs/theme`). It includes:

- YouTube embed styling (`youtube.css` / `youtube.js`)
- mdbook-tabs CSS/JS (`tabs.css` / `tabs.js`)
- Theme toggle (`theme-toggle.js`) and home link (`home-link.js`)
- Favicon

Each book's `book.toml` references these under `additional-css` / `additional-js`.

## Versioning

Product books get one book per minor release — `0.4.0.x` and `0.4.1.x` for StartOS, whose versions carry an extra leading `0`, and `1.5.x` for StartTunnel. Patch releases update their minor's book in place. The other books (`packaging`, `bitcoin-guides`, `support`) are unversioned. `versions.conf` drives `build.sh` and its routing output:

- `book=version` is the product's current version, built from this tree into `docs/<book>/<version>/`.
- `book=version@tag` is an archived version, built from that release tag's tree into `docs/<book>/<version>/`. It gets a banner pointing at the current version and a link from `/archives/`.
- A bare `book` is built straight into `docs/<book>/`.

`MDBOOK_OUTPUT__HTML__SITE_URL` is set per build so mdBook generates correct search indexes and canonical URLs for the versioned path.

### Routing

Unversioned URLs (`/start-os/`, `/start-os/<page>.html`) are stubs that redirect to the current version — or, when the page exists only in an archive, to the newest archive that has it. A stub honours a `?version=` query: the StartOS UI's docs links send the server's version (`?version=0.4.0.4`), and the stub sends it to the archived book whose version prefix matches (`0.4.0.x`), falling back to the default. A path naming a version that isn't built (`/start-tunnel/1.0.x/…`, `/packaging/0.4.0.x/…`) is sent by `404.html` to the same page's unversioned URL.

The copy on `live-docs` determines published entries. Tag sync (`docs-sync-on-tag.yml` through `scripts/docs-versions.mjs`) derives the released StartOS or StartWRT book's current label from its tag, takes StartTunnel and packaging entries from their tagged configuration, and preserves other products' current entries. Archives and site-owned books follow the selected site tree. An archive matching a still-current entry is omitted to prevent its build overwriting the current book.

`/versions.json` publishes each book's current version and archives (`current` is `null` for an unversioned book). The support bot reads it to find the archived books' `llms-full.txt`.

## Build Pipeline

`build.sh`:

1. Fails if a book's `README.md` index does not list its `SUMMARY.md` pages in sidebar order (`scripts/check-indexes.ts`)
2. Wipes and recreates the `docs/` output dir and the gitignored `.archive/` source dir
3. Builds each book in `versions.conf` with `mdbook build` and the matching `SITE_URL`. For an archived version it first fetches the release tag if the checkout lacks it (CI's is shallow), extracts the book and `theme/` from that tag into `.archive/<book>/<version>/` with `git archive`, builds from there, and injects the archive banner into each page
4. Writes the redirect stubs for each versioned book's unversioned URLs (see Routing)
5. Writes `docs/index.html` from `landing/index.html` with each product's current version filled in, `docs/archives/index.html` from `landing/archives.html` with a section per product that has archives (StartOS's also links the `0.3.5.x` site), `docs/versions.json`, and `docs/404.html` from `landing/404.html` with the versions filled in. A static host serves that page for every path it has no file for, and its script applies the remaining routing rules: `/latest/*`, `/packaging-guide`, the one-off legacy redirects, unbuilt version paths, extensionless page URLs, and unknown paths onto the `0.3.5.x` site
6. Runs the llms.txt generator (`scripts/generate-llms-txt.ts`, installing its deps on first use) to produce `llms.txt` (index) and `llms-full.txt` (full content) for each book — an archived version's under `docs/<book>/<version>/`, with versioned page URLs — and for the site, which covers the current books and lists the archives' indexes

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
| `scripts/check-indexes.ts`     | Fails when a book's `README.md` index differs from its `SUMMARY.md`                |
| `scripts/generate-llms-txt.ts` | Produces `llms.txt` (index) and `llms-full.txt` (full content) for LLM consumption |

Run the check with `node scripts/check-indexes.ts` (CI runs it on every pull request), and the generator via `cd scripts && npm run generate-llms-txt` (uses `tsx`) after `./build.sh`, which extracts the archived sources it reads.

## Cross-Book Links

mdBook rewrites intra-book Markdown links but does not reject missing pages or anchors. Links between books use unversioned absolute paths (`/start-tunnel/devices.html`) — the stub `build.sh` writes at that path sends the browser to the current version. They are not validated at build time, so keep them few and correct.

## Further reading

- [README.md](README.md) — what this project is and where the books live
- [AGENTS.md](AGENTS.md) — local setup and how to submit changes
