# AGENTS.md

Operating instructions for AI developers working on the docs-site project (the `projects/start-docs/` project in the `start-technologies` monorepo). `CLAUDE.md` just imports this file. See `ARCHITECTURE.md` for how the build works.

## What this is

This project owns the **site build infra** (`build.sh`, `serve.sh`, `versions.conf`, `theme/`, `scripts/`), the **landing page** (`landing/`), the **Bitcoin Guides** book (`bitcoin-guides/`), and the **Start9 Support** book (`support/`).

## Layout

The StartOS, StartTunnel, Packaging, and StartWRT books are NOT here — they moved into their product dirs:

- StartOS → `../start-os/docs/`
- StartTunnel → `../start-tunnel/docs/`
- Packaging (book name `packaging`) → `../start-sdk/docs/`
- StartWRT → `../start-wrt/docs/`

`build.sh`'s `book_dir()` maps each book name to its source dir. If you're editing content for one of those products, edit it in the product dir, not here — but you can build/preview the whole site from here.

## Build & test (run from `projects/start-docs/`)

- `./build.sh` — builds every book in `versions.conf` into the gitignored `docs/` output dir. Run this to verify the books compile. Check link targets separately; mdBook does not reject missing Markdown pages or anchors.
- `./serve.sh` — build + serve at http://localhost:3000.
- Single book live-reload: `cd <book-src-dir> && mdbook serve -p 3001` (e.g. `cd bitcoin-guides`, or `cd ../start-os/docs`).
- `cd scripts && npm run generate-llms-txt` — regenerate `llms.txt` / `llms-full.txt` (uses `tsx`).

## Gotchas

- Always re-read a file before subsequent edits — a linter/formatter may auto-modify files after changes.
- `theme/` here is the single source of truth; books symlink to it. Edit theme assets here, not in a book's symlinked copy.

## Adding or moving a book

1. Add `book-name=version` to `versions.conf` (build and site routing derive from it; also add the source path to `.github/workflows/docs-deploy.yml`).
2. If the book lives outside this project, add a `book_dir()` case in `build.sh` pointing at its source dir. Books with no mapping default to `<book-name>/` (relative to this project).

## Deployment

**The site serves the `live-docs` branch, not `master`.** GitHub Actions `.github/workflows/docs-deploy.yml` (at the monorepo root) builds and publishes to Start9 Pages on evelyn on push to `live-docs` touching `projects/start-docs/**`, `projects/start-os/docs/**`, `projects/start-tunnel/docs/**`, `projects/start-sdk/docs/**`, or `projects/start-wrt/docs/**` — keep that `paths:` list in step with the set of books. Pages has no server rules: every redirect the site needs is built into the tree by `build.sh` — the stubs for unversioned URLs and the script in `landing/404.html`. A routing change goes there, never into a server config. (`.github/workflows/deploy-docs-pages.yml` on `master` publishes on `live-docs`'s behalf until a tag carries the publish step over, then stands down.)

Content reaches `live-docs` two ways:

- **On a tag.** `docs-sync-on-tag.yml` advances `live-docs` to the tagged tree for the released project, the shared libraries beneath it, and the repo root — **all of `projects/start-docs/`** included, whenever the released product ships a book — then dispatches the deploy. Every other `projects/*` stays on its own release, and a tag behind a release already synced leaves this project, `shared-libs/` and the root where the newer release put them. This is how a book — and any change you make in this project, including `versions.conf`, `build.sh`, and `theme/` — actually goes live. Work here therefore ships on someone else's release: if a site change needs to go out now, PR it to `live-docs` as below.
- **By PR into `live-docs`.** For fixing what is already published. It deploys on merge and is then pushed back to master automatically (`docs-backport.yml`), so don't also write the fix in master.

Because a tag sync overwrites this whole project from the tagged tree, never hand-edit `projects/start-docs/**` on `live-docs` expecting it to survive — land it in master too (the backport does this for you).

## Prerequisites

Use the root toolchain, then:

1. Install [mdBook](https://rust-lang.github.io/mdBook/) (v0.5.2 to match CI) and [mdbook-tabs](https://github.com/niccoloforlini/mdbook-tabs):

   ```
   cargo install mdbook --version 0.5.2
   cargo install mdbook-tabs --version 0.3.4
   ```

2. `build.sh` installs the Node script dependencies itself on first use.

3. From `projects/start-docs/`, build and serve:

   ```
   ./serve.sh
   ```

   This builds all books and serves at http://localhost:3000. For live-reload while editing a single book, run mdBook in that book's source dir:

   ```
   cd ../start-os/docs && mdbook serve -p 3001    # StartOS
   cd bitcoin-guides && mdbook serve -p 3001       # Bitcoin Guides
   ```

## Writing Docs

Each book's pages are flat Markdown files directly under its `src/` (no subdirectory nesting). The sidebar is defined by that book's `src/SUMMARY.md`, which uses `# Part Title` lines for section headers and `---` for separators.

### Page Structure

Every page should have introductory prose (1–2 sentences) between the H1 heading and the first H2. This text is auto-extracted for `llms.txt` to help AI decide which pages to fetch. When creating a new page, add it to the book's `src/SUMMARY.md` or it won't appear in the sidebar or build.

### Admonitions

Use mdBook's built-in admonition syntax:

```markdown
> [!WARNING]
> Do not do this.

> [!NOTE]
> Something helpful.

> [!TIP]
> A useful suggestion.
```

Custom titles are **not supported** — `> [!WARNING] My Title` will break. Put context in the body instead.

### Tabs

Use mdbook-tabs for platform-specific content:

```markdown
{{#tabs global="platform"}}
{{#tab name="Mac"}}
Mac instructions here.
{{#endtab}}
{{#tab name="Linux"}}
Linux instructions here.
{{#endtab}}
{{#endtabs}}
```

`global="..."` makes a tab group's selection sticky across pages (via `localStorage`, mirrored to a `?<global>=<tab>` URL param). Every group sharing a `global` name **must spell shared tab labels identically** — a stored label not present in a group is silently ignored, so mismatched sets fail to sync. So:

- OS pickers use `global="platform"` with these canonical labels: `Mac`, `Windows`, `Linux`, `iOS`, `Android / Graphene`, `ChromeOS`. Use the applicable labels from this set across pages and **don't** promote distros to top-level `platform` tabs — that both breaks the sync and desyncs the picker with other pages. Put distro/version specifics _inside_ the `Linux` tab, either as `####` sub-sections or as a nested tab group with its own `global` (e.g. `global="distro"`), and hoist any shared setup above them so it isn't repeated per distro (see the Root CA guide). A page may omit a platform that genuinely doesn't apply to it — the sync degrades gracefully for that label (a stored selection the page lacks is silently ignored) — but never rename or spell a label differently.
- Anything that isn't a general OS picker (backup targets, cloud providers, …) gets its own `global` — don't overload `platform`.
- Omit `global` for a one-off, page-local group.

Keep the outer picker flat: the only sanctioned nesting is a single distro/version sub-group (with its own `global`) inside one platform tab.

### Cross-Book Links

Links between books use absolute paths; mdBook does not validate their deployed targets:

```markdown
See the [StartTunnel docs](/start-tunnel/).
```

Within a book, use relative paths as usual.
