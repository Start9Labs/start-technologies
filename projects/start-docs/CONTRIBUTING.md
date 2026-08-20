# Contributing

We welcome contributions! Whether you spot a typo or want to suggest new content, fork the [`start-os`](https://github.com/Start9Labs/start-technologies) monorepo and submit a PR. If you're not comfortable with that process, [open an issue](https://github.com/Start9Labs/start-technologies/issues) or reach out via our [community channels](https://start9.com/contact).

This is the docs-site project inside the monorepo. See the [root CONTRIBUTING](../../CONTRIBUTING.md) for the monorepo-wide workflow.

## Documentation

- `README.md` — what this project is
- `ARCHITECTURE.md` — how the site is built and deployed
- `CONTRIBUTING.md` — this file
- `AGENTS.md` — operating rules for AI developers (`CLAUDE.md` is a one-line `@AGENTS.md` import)

## Where the books live

Only the **Bitcoin Guides** book (`bitcoin-guides/`), the landing page, the theme, and the build infra live in this project. The other books live in their product dirs:

- StartOS → `../start-os/docs/`
- StartTunnel → `../start-tunnel/docs/`
- Service Packaging (book name `packaging`) → `../start-sdk/docs/`

`build.sh`'s `book_dir()` maps each book name to its source dir, so the build and the deployed URLs treat all books uniformly.

## Prerequisites

1. Install [Rust](https://rustup.rs):

   ```
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install [mdBook](https://rust-lang.github.io/mdBook/) (v0.5.2 to match CI) and [mdbook-tabs](https://github.com/niccoloforlini/mdbook-tabs):

   ```
   cargo install mdbook --version 0.5.2
   cargo install mdbook-tabs --version 0.3.4
   ```

3. Install [Node.js](https://nodejs.org/) (v22+) and the script dependencies (only needed to regenerate `llms.txt`):

   ```
   cd scripts && npm install && cd ..
   ```

4. From the `docs/` project, build and serve:

   ```
   ./serve.sh
   ```

   This builds all books and serves at http://localhost:3000. For live-reload while editing a single book, run mdBook in that book's source dir:

   ```
   cd ../start-os/docs && mdbook serve -p 3001    # StartOS
   cd bitcoin-guides && mdbook serve -p 3001       # Bitcoin Guides
   ```

## Building

- `./build.sh` — build all books into the `docs/` output dir
- `./serve.sh` — build + serve on http://localhost:3000
- `cd <book-src-dir> && mdbook serve -p 3001` — live-reload a single book
- `cd scripts && npm run generate-llms-txt` — regenerate `llms.txt` files

## Testing

`./build.sh` is the primary check — mdBook fails the build on broken intra-book links, so a clean build verifies your change compiles.

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

`global="..."` makes a tab group's selection sticky across pages (via `localStorage`, mirrored to a `?<global>=<tab>` URL param). Every group sharing a `global` name **must use identical tab labels** — a stored label not present in a group is silently ignored, so mismatched sets fail to sync. So:

- OS pickers use `global="platform"` with exactly these labels: `Mac`, `Windows`, `Linux`, `iOS`, `Android / Graphene`. Keep this set identical across pages and **don't** promote distros to top-level `platform` tabs — that both breaks the sync and desyncs the picker with other pages. Put distro/version specifics _inside_ the `Linux` tab, either as `####` sub-sections or as a nested tab group with its own `global` (e.g. `global="distro"`), and hoist any shared setup above them so it isn't repeated per distro (see the Root CA guide).
- Anything that isn't a general OS picker (backup targets, cloud providers, …) gets its own `global` — don't overload `platform`.
- Omit `global` for a one-off, page-local group.

Keep the outer picker flat: the only sanctioned nesting is a single distro/version sub-group (with its own `global`) inside one platform tab.

### Cross-Book Links

Links between books use absolute paths since mdBook only validates intra-book links:

```markdown
See the [StartTunnel docs](/start-tunnel/).
```

Within a book, use relative paths as usual.
