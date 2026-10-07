# AGENTS.md

Agent/developer operating rules for the **start-technologies monorepo root**. This repo is the monorepo for all Start9 products. `CLAUDE.md` is a one-line `@AGENTS.md` import — do not edit it.

This file owns the shared contributor workflow and operating rules. See [README.md](README.md) for product context and [ARCHITECTURE.md](ARCHITECTURE.md) for the layout.

**Keep these docs current.** Every scope carries `AGENTS.md` / `ARCHITECTURE.md` / `README.md` (and `CLAUDE.md`, a one-line `@AGENTS.md` import). When a change makes a documented statement about structure, conventions, the build/test/release flow, or product context inaccurate, correct it in the **same change** — never defer. Leave accurate text alone. These docs are **hierarchical**: each scope's docs cover only what is specific to it and must not repeat anything already stated at a higher scope (e.g. commit/PR conventions live only in this root `AGENTS.md`).

**Required scope rules belong in `AGENTS.md`.** It is plain Markdown, with no import syntax; a link does not load its target. Inline the workflow rather than hiding it behind a contribution guide. For shared skills and topical references, state explicitly when they must be read instead of duplicating their contents. `CLAUDE.md` stays a one-line `@AGENTS.md` import.

**Contributor instructions live in `AGENTS.md` at every scope.** Do not create separate contribution guides. Each `AGENTS.md` must have a sibling `CLAUDE.md` containing exactly `@AGENTS.md` and a newline. Put project context in `README.md`, structural reference in `ARCHITECTURE.md`, and required workflows and rules here.

**Documentation follows accuracy, not the diff.** For user-facing books and developer-facing references, change existing text only when the change makes it inaccurate or misleading. If the docs remain accurate, leave them unchanged: touching a feature does not make it more important or warrant a special callout, caveat, or explanation of the fix. The exception is a new feature readers would not otherwise discover and that would have belonged in the docs had it existed when they were written; document it in the appropriate existing page or section, at the same level of emphasis as comparable features. Required documentation changes ship with the code in the **same change**, in the owning product's `docs/` directory (e.g. `projects/start-os/docs/`, `projects/start-tunnel/docs/`, `projects/start-sdk/docs/`).

**Changelog requirements are separate.** Any change that alters user-visible behavior **must** add a fragment at `projects/<product>/changelog/<tier>-<kind>-<descriptive-name>.md` in the **same change**, whether or not its documentation needs editing. Use `patch`, `minor`, or `major` for the impact tier and lowercase `added`, `changed`, `deprecated`, `removed`, `fixed`, or `security` for the kind. The body is Markdown bullets without headings. Internal tooling changes need no product fragment. `CHANGELOG.md` is generated release history; contributors do not edit it.

**Book authoring conventions.** The conventions for authoring **any** product book — mdBook versions, admonitions, tabs, `SUMMARY.md`, the shared `theme/` — live in [`projects/start-docs/AGENTS.md`](projects/start-docs/AGENTS.md). That project is a sibling, not an ancestor, so nothing loads it for you: read it before editing book pages anywhere in the repo.

**The highest pending fragment tier determines the next version.** Freshly query origin before deciding release state — `git fetch --tags origin` or `git ls-remote --tags origin '<product>/v*'`. The highest stable `<product>/v<version>` tag is the baseline; prerelease tags do not advance it. Update the canonical manifest to that baseline plus the highest pending tier: `patch` increments patch, `minor` increments minor and resets patch, `major` increments major and resets minor and patch. StartOS keeps a leading `0` outside SemVer: `0.4.0.3` is effectively `4.0.3`, with a zero patch omitted (`0.4.1`, not `0.4.1.0`). With no pending fragments, keep the released version; before a product's first release, choose its initial version explicitly. PR CI enforces the calculation for products whose fragments or canonical manifests changed; push CI checks all products. `manage-release.sh pre-check` and `tag` enforce it for the release being cut. They use `scripts/changelog.py check-version projects/<product>`. A version bump pairs with its fragments and release notes. If a change refines a feature still awaiting release, update that feature's fragment rather than describing a fix to something users never received.

**An entry says what changed; the commit says why.** It is read by someone deciding whether an upgrade affects them and wanting a high-level sense of what the release did — so an entry runs as long as it takes to convey that, and stops there. A breaking change that needs a migration instruction gets one; a fix nobody could observe does not need its mechanism explained. What never belongs is the material that explains the change to whoever reviewed it: the root cause, the implementation detail, the alternatives you rejected, the story of how you found it. That goes in the commit message and the PR body, where it stays attached to the diff.

**Read down into what you touch.** When you work in a subdirectory, first read its `AGENTS.md` — and any further nested `AGENTS.md` on the way down to the files you're editing — before changing anything. Each scope's docs assume you've read the scopes above it, so a subdir's `AGENTS.md` adds only its own rules on top of this root.

## Layout

Each product lives under `projects/` as a thin wrapper; the bulk of the code lives in the top-level shared libs (`shared-libs/`).

- `projects/start-os/` — OS product. Rust bins `startbox` + `start-container` (`src/bin/*.rs`), `web/` (Angular UI + setup-wizard), `container-runtime/` (Node LXC service runtime), bin build scripts + OS image build (`build/`), Debian control (`debian/`), VM-setup `assets/`, `backup-fs/`, `docs/`, `*.service`.
- `projects/start-cli/` — `start-cli` bin (`src/main.rs`); thin wrapper over `start-core`.
- `projects/start-registry/` — `registrybox` bin; registry API and asset server, consumed by the shared marketplace UI lib.
- `projects/start-tunnel/` — `tunnelbox` bin + `web/` (StartTunnel UI).
- `projects/start-wrt/` — StartWRT, an OpenWrt-based router OS. Rust backend (`startwrt` bin: RPC daemon + CLI, crates `ctrl`/`uciedit`/`uciedit_macros`) building on shared `start-core`; an Angular `web/` UI (a project in the root Angular workspace) embedded into the binary; a build-managed `openwrt/` workspace (pinned upstream OpenWrt tarball + the Start9 delta in `openwrt-patches/` + `openwrt-overlay/`); flashable image for the SpaceMiT K1.
- `projects/start-sdk/` — `@start9labs/start-sdk` (source in `lib/`; imports the shared `@start9labs/start-core` lib and bundles it into its published `dist/`) + `Makefile`/`s9pk.mk` + `docs/` (packaging mdbook).
- `projects/brochure-marketplace/` — public marketplace Angular app (deploys to marketplace.start9.com).
- `projects/start-docs/` — the documentation website (build infra + landing + Bitcoin guides; each product's own book lives in its `docs/`).
- `shared-libs/crates/start-core/` — the **entire** Rust backend lib (package `start-core`, lib name `start_core`). All six bins depend on it.
- `shared-libs/ts-modules/` — shared **TypeScript** modules (the common thread is just that they are TS — not Angular-specific). These are the Angular libs `shared/` (`@start9labs/shared`) and `marketplace/` (`@start9labs/marketplace`), plus the non-Angular `start-core/` (`@start9labs/start-core`: SDK core types/ABI/effects/OS bindings, the TS projection of the `start-core` crate, consumed by web and bundled into the SDK; versionless, not published separately). The Angular workspace is rooted at the repo root (`angular.json`/`package.json`). Product apps reference the libs by package name.
- Agent skills, in the Agent Skills format, live in two places by who reads them. `.claude/skills/` holds the skills for work in this repo (`start9-frontend`); Claude Code loads them as project skills and `.agents/skills` symlinks the directory for Codex. `projects/start-sdk/docs/skills/` holds the packaging skills (`package-service`): guide content that reaches `live-docs` the way a page does, and that packaging workspaces and helix read from their guide checkout. Nothing at the repo root refers to them: a packaging skill is reached through a packaging workspace, never installed on its own. A skill that is Start9's own procedure rather than doctrine for every packager ships with helix, not here. A skill states what to do, never which tool does it — it is read by agents with different tool sets.
- Top level also holds the shared build infra (`build/`, `Makefile`), `apt/`, the shared `debian/build.sh`, `scripts/` (maintainer release tooling — `manage-release.sh <subcommand> <project>` drives a product through pre-check/tag/release/sign/publish; `deploy-migration-payload.sh` publishes a 0.3.5.1 → 0.4.0 migration OTA payload to a legacy registry), `rfcs/` (protocol drafts), and `shared-libs/crates/patch-db/` (first-party crate, consumed by `start-core` and web).

## Collaboration

- [Matrix](https://matrix.to/#/#dev-startos:matrix.start9labs.com)
- Security issues: [security@start9.com](mailto:security@start9.com) — see [SECURITY.md](SECURITY.md)

## Environment Setup

> Debian/Ubuntu is the only officially supported build environment.
> MacOS has limited build capabilities and Windows requires [WSL2](https://learn.microsoft.com/en-us/windows/wsl/install).

The shared toolchain below is enough to build the Rust bins and the web apps. **Individual products need more** — most notably the StartOS OS image, which adds multi-arch emulation and image-packaging tooling. See each product's `AGENTS.md` for its additional system dependencies.

**Web-UI work skips most of this.** The Angular front ends build and run standalone against mock data — they need only Node 24 and Make, no Rust, Docker, or OS-image tooling. See [`shared-libs/ts-modules/AGENTS.md`](shared-libs/ts-modules/AGENTS.md).

```bash
# Common build tooling
sudo apt update
sudo apt install -y ca-certificates curl gpg build-essential git \
  sed grep gawk jq gzip brotli rsync python3

# Rust (stable; rustfmt runs in a pinned-nightly container — see Formatting)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh # proceed with default installation

# Node.js 24 (shared web toolchain)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/master/install.sh | bash
source ~/.bashrc
nvm install 24
nvm use 24
nvm alias default 24 # this prevents your machine from reverting back to another version
```

Install Docker using the [instructions for the host distribution](https://docs.docker.com/engine/install/). OS-image and cross-compilation recipes use it; local `.s9pk` packaging can also use Podman.

### Cloning

```bash
git clone https://github.com/Start9Labs/start-technologies.git
cd start-technologies
```

## Building

**Use `make` recipes when they exist** rather than re-deriving underlying commands. This is a monorepo: one root Cargo workspace and one Angular workspace, both rooted at the repo root. The root `Makefile` is a thin orchestrator (it `include`s each product's `build.mk`) — run `make` with no target to print a help summary; the default target is `help`. Run build commands from the repo root.

- **A single Rust bin:** `cargo build -p <crate> --bin <bin>` — crates are `start-os` (`startbox` / `start-container`), `start-cli`, `start-registry` (`registrybox`), `start-tunnel` (`tunnelbox`), and `startwrt-core` (`startwrt`).
- **A whole product** (bins + UI + packaging) has its own `make` targets and build instructions in its `AGENTS.md`:

| Product                                | Primary build target                                                                                          | Build & deploy docs                                                               |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| StartOS (OS image, UIs, device deploy) | `make start-os`                                                                                               | [`projects/start-os/AGENTS.md`](projects/start-os/AGENTS.md#contributor-workflow) |
| start-cli                              | `make start-cli`                                                                                              | [`projects/start-cli/AGENTS.md`](projects/start-cli/AGENTS.md)                    |
| start-registry                         | `make start-registry`                                                                                         | [`projects/start-registry/AGENTS.md`](projects/start-registry/AGENTS.md)          |
| StartTunnel                            | `make start-tunnel`                                                                                           | [`projects/start-tunnel/AGENTS.md`](projects/start-tunnel/AGENTS.md)              |
| StartWRT                               | `make start-wrt` (`make start-wrt-image` for the full OpenWrt image — hours, fetches the pinned OpenWrt tree) | [`projects/start-wrt/AGENTS.md`](projects/start-wrt/AGENTS.md)                    |
| Start SDK                              | `make bundle` (from `projects/start-sdk`)                                                                     | [`projects/start-sdk/AGENTS.md`](projects/start-sdk/AGENTS.md)                    |
| Web (shared libs + app UIs)            | `npm run build:ui`                                                                                            | [`shared-libs/ts-modules/AGENTS.md`](shared-libs/ts-modules/AGENTS.md)            |

`make start-core-ts-bindings` regenerates the TypeScript bindings from the Rust types, and `make clean` removes all compiled artifacts (ask before running it). Cross-layer changes (Rust → bindings → SDK → web/runtime) are described in [ARCHITECTURE.md](ARCHITECTURE.md#build-pipeline).

### Build configuration

The shared build recipes accept these environment variables. StartWRT pins its router architecture in `projects/start-wrt/build.mk` rather than deriving it from `PLATFORM`:

| Variable             | Description                                                                                        |
| -------------------- | -------------------------------------------------------------------------------------------------- |
| `PLATFORM`           | Target platform (e.g. `x86_64`, `aarch64`, `riscv64`). For non-OS products it only derives `ARCH`. |
| `ENVIRONMENT`        | Hyphen-separated feature flags; the available options depend on the product.                       |
| `PROFILE`            | Build profile: `release` (default) or `dev`.                                                       |
| `GIT_BRANCH_AS_HASH` | Set to `1` to use the git branch name as the version hash (avoids rebuilds).                       |

Each product's `AGENTS.md` documents the `PLATFORM` values and `ENVIRONMENT` flags it actually supports.

## Testing

```bash
make test                    # all tests
make start-core-test         # Rust (shared-libs/crates/start-core)
make start-sdk-test          # SDK
make backup-fs-test          # backup-fs library tests except the /dev/fuse suite
make container-runtime-test  # container runtime
make start-os-scripts-test   # StartOS shell scripts (projects/start-os/build/tests)
make start-wrt-test          # StartWRT Rust crates
make release-notes-test      # changelog fragments, versions, release notes, signed commits

# Run a specific Rust test
cd shared-libs/crates/start-core && cargo test <test_name> --features=test
```

Each product's `AGENTS.md` covers its own scoped tests.

## Formatting

Three tools, one config each at the repo root: **rustfmt** (`rustfmt.toml`) for Rust,
**prettier** (`.prettierrc.json`) for TS/JS/HTML/SCSS/Markdown/YAML/JSON, and **taplo**
(`taplo.toml`) for TOML.

Every prettier invocation passes `--config .prettierrc.json`. The package template's
`.prettierrc` names the published SDK's config, which resolves only in a scaffolded package,
so config discovery fails on any template file.

```bash
make format          # format the whole repo
make format-check    # read-only check (what CI runs)
```

rustfmt uses options that are still nightly-only, so to keep output identical for
everyone it runs in a pinned-nightly container — `build/fmt/fmtenv.Dockerfile`, which
adds the pinned nightly to `start9/cargo-zigbuild` (the same image the Rust build
uses) and is built on first use. prettier and taplo are pinned via npm
devDependencies and run natively. To bump a version, edit the Dockerfile's
`RUSTFMT_TOOLCHAIN` (rustfmt) or `package.json` (prettier / `@taplo/cli`).

If you already have the pinned nightly installed and want to skip Docker:

```bash
FMT_NATIVE=1 make format
```

Or scope Rust formatting to one crate (still through the container):

```bash
make start-core-format   # shared Rust crates
make start-cli-format    # also start-registry-format / start-tunnel-format / start-os-format / start-wrt-format
make web-format          # prettier over the whole repo
```

Run the formatters before committing. A git pre-commit hook (husky + lint-staged)
auto-runs prettier on staged files once you've run `npm ci`, so a missed format
won't reach CI; it no-ops when dependencies aren't installed. CI enforces
formatting regardless: a fast `prettier --check` gate runs on every pull request
(including docs-only ones) and blocks the slower jobs, with `make format-check` as
the source of truth.

## Branches and channels

- **`master` is always shippable, and every push to it publishes.** Only merge work that is finished and tested: a master push indexes StartOS images into the alpha registry, publishes `.deb`s into the `alpha` apt suite, and — once the `STARTWRT_ALPHA_REGISTRY` repo variable is set — StartWRT images into its alpha registry. There is no staging step between merging and publishing. The SDK is the deliberate exception: an npm version can never be republished, so it publishes only on release.
- **Work that can't land piecemeal goes on `integration/<topic>`.** Land the pieces there as ordinary PRs, soak the branch as a unit, and delete it on merge. The PR merging it into master is the last gate before it ships, so CI builds the full flashable image matrix on that PR specifically — the bypass lives in `startos-iso.yaml`'s `changes` job and start-wrt's `image` job, both keyed on `head_ref` starting with `integration/` and `base_ref` being `master`.
- **Re-indexing a version into a registry requires removing it first.** `registry os asset add` upserts the platform slot but **rejects** a write whose blake3 differs from the commitment already there (`registry/os/asset/add.rs`, `registry.os.asset.commitment-mismatch`) — so publishing a second build under a version number that already holds one fails, even though `registry os version add` itself is a harmless upsert. Alpha rolls the same version on every master push, so both deploy jobs run `registry os version remove` first. Anything that re-publishes a version in place must do the same.
- **Alpha is the source a release is promoted from, so a master push must build release-equivalent artifacts.** `os promote` copies each asset's URL and signed commitment through verbatim (`registry/os/promote.rs`), so the bytes a master push uploads are the bytes production eventually serves — from the same S3 key. Two consequences: `ENVIRONMENT` is empty on master (only PRs and `integration/*` default to `dev`), and alpha images must **not** be given a separate S3 prefix, or promoted production assets would point inside it. It also means an S3 key, once published, is immutable: both deploy jobs refuse to overwrite an object that already exists and fail listing every conflict, since replacing those bytes would leave an already-promoted signature validating against content that no longer matches. Re-running a deploy for a commit that was published before therefore needs a dispatch with `force-overwrite`.

- **The debs promote out of `alpha` too.** `release start-cli` / `start-tunnel` / `start-registry` stage from the `alpha` suite (`pull-alpha`), not from a CI run, so the packages testers have been running are the ones that reach `stable`. The whole chain is verified against the suite's signature: `InRelease` against [`apt/start9-alpha.gpg`](apt/start9-alpha.gpg), `Packages` against the hash the signed Release commits to, and each `.deb` against the hash that index commits to. Nothing about the promotion trusts plain HTTPS — the pool key is stable across builds, so an unverified download could be swapped by a republish mid-promotion. It also fetches from the **S3 origin, not the `*.cdn.*` host** in `apt/*.list`, and sends no-cache headers: a cached `InRelease` is still a validly signed one, so every check would pass while promoting whatever build the edge was holding. Signatures prove authenticity, not freshness — don't align those two URLs. That leaves replayed signed metadata theoretically promotable, and the answer to that is **not** a `Valid-Until` on the suite: expiring a rolling channel on a timer unrelated to release cadence breaks `apt update` for everyone during any quiet period. What actually guards it is that the operator is shown the commit being tagged, where a replay appears as an unexpectedly old hash.
- **A deb release tags the commit alpha built, which is often not `HEAD`.** The build is identified by the `Git-Hash` control field [`debian/build.sh`](debian/build.sh) writes (`Version` cannot identify it — every master build of a version publishes under the same one). Each product's workflow is **path-filtered**, so master advances on changes elsewhere without rebuilding that product, and `alpha` legitimately holds a build of an older commit while no build of `HEAD` exists or ever will. Rather than demanding one, `pull-alpha` **adopts** alpha's commit and tags there — the tag has to point at the commit that produced the artifact. It says so on the way past, and prints the `git checkout` to put a tree on that commit; `./scripts/manage-release.sh alpha-commit <project>` prints the same hash for scripting. Adoption is confined to a commit already in the current branch's history, and an explicit `COMMIT=` is never overridden — it fails instead, because that is someone asserting a different intent. start-cli's per-triple binaries are the one thing that cannot be promoted — they are only ever GitHub release assets, so that half still needs the run. To put a CI build on a server without building locally: `make start-os-update-from-gha REMOTE=start9@<ip>` (latest master build) or `RUN_ID=<id|url>` for a specific run.
- **`live-docs` is what every product has published, and is not a development branch.** docs.start9.com serves it, and `start-cli s9pk init-workspace` clones it — a packaging workspace reads its guide, package template, and SDK source from that branch, so what is on it must be true of shipped software. master's `docs/` books describe the version that has not shipped yet. Never merge a docs change into `live-docs` that isn't already true of the _published_ software.
- **A pull request to `live-docs` may only change published book files.** The code on that branch is written by [`docs-sync-on-tag.yml`](.github/workflows/docs-sync-on-tag.yml) alone, on a release — a PR that carried code would put a tree there that no release produced, and [`docs-backport.yml`](.github/workflows/docs-backport.yml) would then push it onto master unattended, unreviewed as a code change. [`live-docs-guard.yml`](.github/workflows/live-docs-guard.yml) fails any PR touching a path outside `projects/<product>/docs/` and `projects/start-docs/`, and rejects `projects/start-sdk/docs/package-template/` as well. That lives under `docs/` but is the code `s9pk init-package` scaffolds from, read out of the workspace's checkout at scaffold time rather than built into `start-cli` — so a merge there changes what every new package is built from at once, without facing the `manage-release.sh pre-check start-sdk` gate on its SDK pin.
- **A book change ships with its code on `master`; a book-only fix goes to `live-docs`.** This routing governs the **published books only** — `projects/<product>/docs/**` and `projects/start-docs/**`, the paths [`docs-deploy.yml`](.github/workflows/docs-deploy.yml) triggers on. Every other markdown in the repo — `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, at any scope — is ordinary repo content that nothing publishes, and lands on master like code. For the books, the test is whether the change accompanies code. A book edit describing a change you are making belongs in the same PR as that change, on master, and reaches the site when that product is tagged. A change that only corrects a book — stale text, a dead link, a wrong path, anything already wrong on the published site — belongs on `live-docs`: it deploys on merge, and [`docs-backport.yml`](.github/workflows/docs-backport.yml) then pushes the same commit to master unattended. A book-only PR opened against master instead sits unpublished until the next tag. Where part of such a change touches book text that exists **only** on master (a section describing unshipped behavior), split that part into its own master PR — never write the same fix in both places, because one of the two will conflict and the conflict falls to a human.
- **A green `Conflict Markers` check does not mean a backport is clean.** Where git cannot merge a file line by line — a page deleted on one side, an image changed on both — the backport keeps one side and leaves no marker, so compare the PR against `live-docs` before merging it.

## Releases

- **Changelog compilation is shared by tag CI and the release script.** `docs-sync-on-tag.yml` uses the `docs-publish` App to compile tagged fragments under `## [<version>]`, write the generated history to `master`, and consume only fragments unchanged since the tag. It retains that generated history on `live-docs`. Re-running an unchanged tag is idempotent; changing an already-compiled release's fragments fails rather than overwriting history. `manage-release.sh` renders the same compiler output and attaches it as `CHANGELOG.md` to the GitHub release.
- **Cutting `<project>/v<version>` is what advances `live-docs` for that product.** [`.github/workflows/docs-sync-on-tag.yml`](.github/workflows/docs-sync-on-tag.yml) takes the tagged tree **whole**, then puts every _other_ `projects/*` back on the release it is on. So a tag moves its own project, the shared libraries beneath it (`shared-libs/`), and the repo root — and nothing another product owns. The shared `projects/start-docs/` site tree rides along only with a product that ships a book, `versions.conf` included, so `versions.conf` must already name the version you are about to publish **before** you tag; it is hand-maintained and the sync will not fix it for you. A bookless release (start-cli, start-registry) still advances its own project and the shared tree; it just leaves the site alone, so no deploy fires. Nothing moves backwards: a tag behind a release already synced — a deb release adopts the commit alpha built, often older than a tag cut before it — still advances its own project but leaves the shared trees where the newer release put them, and a tag behind its own product's last release publishes its book alone.
- **Cut every release with [`scripts/manage-release.sh`](scripts/manage-release.sh)** — `./scripts/manage-release.sh release <project>` (`start-os`, `start-cli`, `start-tunnel`, `start-registry`, `start-sdk`, `start-wrt`); `--help` lists the individual subcommands and env vars. A product's version is read from its manifest (`Cargo.toml`, or `package.json` for the SDK), and its git tag / GitHub release is `<project>/v<version>`. **StartOS is the exception:** its version carries a revision segment SemVer cannot express (`0.4.0.1`), so the **root `package.json`** holds it and `projects/start-os/Cargo.toml` carries only a `0.4.0-rev.1` label — never a comparand, since a SemVer prerelease sorts _below_ its release. Read it via `build/env/version.sh`; see [`shared-libs/crates/start-core/VERSION_BUMP.md`](shared-libs/crates/start-core/VERSION_BUMP.md).
- **A release's curated notes live at `projects/<product>/release-notes/<version>.md`.** Structure: a lede, `## Highlights`, an optional `## Important`. Put pre-update instructions in an optional sibling `<version>.pre-update.md`, headed `## ⚠️ Before You Update`. `manage-release.sh` prefixes that companion to the main notes for the registry and GitHub release, appending the full-changelog link to `## Highlights`. The StartOS image packages the main notes alone for its post-update welcome. `pre-check` fails without the main file. Keep already-released notes as published. Fragments carry every notable change; these notes carry the ones a user would decide on. The default full-changelog link renders the released version's section in master's `CHANGELOG.md`, populated by tag CI; the GitHub release also carries a compiled attachment. An explicit `CHANGELOG_REF` (used by alpha) links to that ref's `projects/<product>/changelog/` directory.
- **A metadata-only master change must publish matching channel metadata.** `pull-alpha` tags the commit that built `start-cli`, `start-tunnel`, or `start-registry` and refuses to publish notes or fragments that differ from that commit; StartWRT's alpha registry records the notes from its build commit. Deb workflows therefore re-include each product's `release-notes/**` and `changelog/**` in `push.paths` after the global Markdown exclusion. StartWRT re-includes its release notes. Keep these exceptions out of `pull_request.paths`: they are not binary inputs.
- **Never invoke a product's publish step directly** (`make publish`, an upload, a registry index). The pipelines differ per product — npm for the SDK, apt + GitHub release for the debs, S3 + registry promotion for the OS and StartWRT — but all of them run the **idempotent steps (tag, GitHub release) _before_ the irreversible one**. Skip the pipeline and you strand a released version with no tag and no GitHub release, which for npm cannot be undone (`pre-check` then refuses the version and npm won't republish it). This is exactly how start-sdk 2.0.4 and 2.0.5 shipped, and they had to be backfilled. Reach for individual subcommands only to repair a partial release.
- **Release from a merged, up-to-date `master`.** The tag is a claim that a commit on `master` produced the artifact, so cut it where that's true. Nothing enforces this — publishing out of band from an unmerged branch is deliberately still possible, and sometimes the right call — but it is a **debt, not a shortcut**: the commit you published from will be squashed or orphaned when the branch merges, leaving the tag nowhere honest to point. If you take it, you owe the follow-up in the same sitting — merge the branch, then tag and release at the resulting `master` commit, having checked that its shipped subtree still matches the artifact you published. start-sdk 2.0.5 went out this way and had to be reconstructed after the fact.
- Per-product prerequisites and specifics live in that product's scope — e.g. [`projects/start-sdk/AGENTS.md`](projects/start-sdk/AGENTS.md#cutting-a-release), [`projects/start-wrt/AGENTS.md`](projects/start-wrt/AGENTS.md#cutting-a-release).

## Filing issues

- **The issue forms are the human path, and you will never see them.** `.github/ISSUE_TEMPLATE/*.yml` binds the web UI only — `gh issue create` does not consult a template, so nothing sets the type or the label on your behalf. Pass both yourself (`gh issue create --type Bug --label StartOS`) and write the body from the spec below; don't go open a form to find out what it wants. An issue filed without both is gated: [`issue-triage.yml`](.github/workflows/issue-triage.yml) labels it `needs-triage` and lifts the gate once both are set — until then it is outside every per-project view.
- **Type is a GitHub issue type, not a label.** `Bug`, `Feature`, or `Task`, defined at the org level and passed as `--type`. There is no `bug` or `enhancement` label — don't invent one.
- **Take the project label from this exact set.** The casing is inconsistent and is matched literally: `StartOS`, `start-cli`, `StartSDK`, `StartWRT`, `StartTunnel`, `start-registry`, `start-docs`, `brochure`, and `repo` (build, CI, and release tooling). One label is the norm — reach for a second only when a defect genuinely spans two products. The shared libraries — `start-core`, `patch-db`, `exver`, `ts-modules` — have none of their own, so label them with the product the defect is most visible in.
- **The status labels are not the filer's.** `Approved` means signed off and ready for a PR; `Known Solution` means the fix is identified but unimplemented; `verified` means Start9 has confirmed the defect is real — all three are the maintainer's. `needs-triage` is the workflow's: it applies and removes it itself, so never set or clear it by hand.
- **Don't pass `--assignee`.** [`.github/workflows/issue-triage.yml`](.github/workflows/issue-triage.yml) routes the issue to its owner from the project label you set, and features to the maintainer regardless of project. Getting the label right is therefore what gets the issue in front of the right person: a gated issue sits with `TRIAGE_OWNER` until it is typed and labelled, then moves to its project's owner.
- **Title the issue as the finding, not the symptom.** Once you have traced the cause, `<what breaks> — <why> (<file:line>)` beats a bare description of what you saw.
- **A bug in a packaged service is not a bug in this repo.** Defects in Bitcoin Core, LND, Nextcloud and the rest belong in that package's own `*-startos` repo. File here only when the fault is in StartOS, the SDK, or another product in this monorepo.

Write the body with these headings verbatim. Omit a section you have nothing for rather than filling it with "N/A":

```markdown
### Environment

<the facts listed for this product below>

### What happens

### What should happen instead

### Steps to reproduce

<omit when you found this by reading code — say so under Root cause instead>

### Root cause / code pointers

<`path/to/file:line` and the mechanism, only once you have verified it. Omit rather than guess: a confident wrong cause costs triage more than an empty section. Human filers almost never supply this, so it is where you add the most — and where you do the most damage by bluffing.>

### Logs & evidence
```

What `### Environment` carries, by project:

- `StartOS` — version and git hash (`start-cli -H <host> git-info`), architecture and image variant, server hardware, and the area (web UI, service runtime, networking, backup/restore, update)
- `start-cli` — `start-cli --version` and `start-cli git-info`, platform, install method, the exact failing invocation, and the target server's version for a remote call
- `StartSDK` — the `@start9labs/start-sdk` version, the package being built, Node version, and the StartOS version for a runtime failure
- `start-registry` — which registry, `registrybox` version if self-hosted, and the client used (marketplace tab, brochure, `start-cli registry`, direct RPC)
- `StartTunnel` — `start-tunnel --version` and `dpkg -l start-tunnel`, VPS provider and OS, whether the host is behind NAT, and host firewall state
- `StartWRT` — the image release or git hash, the board, and the relevant `uci show <package>` output
- `brochure` — page URL, browser and OS, and the registry being browsed
- `start-docs` — page URL, which book, and the source file under `projects/<product>/docs/`

## Opening PRs

- **Label every PR with the project(s) it modifies.** Nothing labels it for you: [`issue-triage.yml`](.github/workflows/issue-triage.yml) is bound to `issues:` alone, and no workflow reads a PR's diff. Pass them when you open it — `gh pr create --label StartOS --label StartSDK` — or add them after with `gh pr edit <n> --add-label repo`. Take them from the same set as an issue, listed under [Filing issues](#filing-issues); `gh` fails on a label the repo doesn't have, and the casing is matched literally.
- **No "Test plan" section in the body.** Summarize what the change does and why, plus executed verification and its results. CI supplies an additional test record; a list of checks someone should run does not.
- **The diff decides, so a PR takes as many labels as it needs.** An issue carries the one product a defect surfaces in; a PR carries every project whose files it changes, because that is what tells a reviewer and a release what a merge can break. Build, CI, and release tooling — `.github/`, `build/`, `scripts/`, `Makefile`, `debian/`, `apt/`, and the repo-root docs — is `repo`. A `shared-libs/` change has no label of its own: label the products whose behavior it changes.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**

- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation only
- `style` - Formatting, no code change
- `refactor` - Code change that neither fixes a bug nor adds a feature
- `test` - Adding or updating tests
- `chore` - Build process, dependencies, etc.

**Examples:**

```
feat(web): add dark mode toggle
fix(core): resolve race condition in service startup
docs(repo): update contributor workflow
refactor(sdk): simplify package validation logic
```

## Licensing

This repository is MIT. By contributing you agree your work is licensed under
the MIT License in [LICENSE](LICENSE), and that you have the right to license it
that way.

If a change vendors third-party code into the tree, it must keep the upstream
copyright notice and be added to [NOTICE.md](NOTICE.md) in the same PR. Prefer a
build-time dependency over a checked-in copy.

Don't link GPL or AGPL code into a StartOS binary: the combined work would have
to be conveyed under those terms. LGPL and MPL-2.0 code may be linked — both
permit it — but each carries obligations we then owe downstream, so raise it
before adding one. `deny.toml` encodes which licenses are accepted.

## Code style

**Frontend work uses the `start9-frontend` skill.** Read [`.claude/skills/start9-frontend/SKILL.md`](.claude/skills/start9-frontend/SKILL.md) and its relevant references before writing or reviewing Angular components, templates, styles, forms, dialogs, routes, state, or i18n. The skill owns frontend conventions and stack-version facts; neighbouring code may predate them. Verify Taiga APIs against its reference rather than guessing. Update the skill in the same change when conventions or stack versions change; sibling Start9 repositories consume this copy through symlinks.

- **Default to zero comments.** A comment must protect a caller or reader from a mistake the code cannot — a wrong input, a misread return, a hazard that bites later. If nobody would go wrong without it, there is no comment, on a self-explanatory helper most of all. Try the rename or the restructure that would make the comment unnecessary first.
- **A comment is a predicate, not a paragraph.** `Saturates at 255 rather than wrapping.` One clause, one restriction, stated bare — twelve words is a budget, not a target. Never enumerate the cases that trigger the rule: `Bad input zeroes the accumulator.`, not a list of the inputs that are bad. Never `only X counts`; state what counts. An error path that degrades into the default is the default — don't narrate it. Where the declaration supplies the subject, a bare clause is complete.
- **Leave the signature out of the prose.** The parameter names sit on the screen next to the comment; echoing them repeats the declaration. Say what an argument means by its role, not its identifier. A formal term — a checksum, a timestamp, a `Host` header — is not re-explained; the reader is owed only what the term does not imply.
- **State the fact, don't argue it.** No `so`, `because`, or `which means`; no clause about what other code is thereby spared. Keep the precise noun, drop the explanation of it: `Filters the application protocols the client asks for. Unset filters none of them.`
- **Say what is true, not when you wrote it.** No `currently`, `new`, `for now`, or `temporary until X lands`; no `// was: …` changelog; no pointer at a PR, a ticket, or a caller. Git holds the history, and a `// TODO` belongs in an issue (see [Filing issues](#filing-issues)). Write nothing about what the code or the product does _not_ do — a capability that turned out to be unnecessary, a behaviour that was removed, a check that is skipped. That holds in user-facing copy exactly as it holds in code.
- **Published API documentation is not commentary.** Document exported SDK types and public crate items accurately for callers reading them in an editor. A module or type overview may need more than one clause; keep implementation commentary to the budget above. A `///` on a `#[ts(export)]` type also changes the generated TypeScript binding — regenerate it.
- **The comments around you are not a style reference.** Most of the doc comments in this repo were written by an agent, including many that blame attributes to a maintainer. Follow these rules, not the neighbours.
- **Fix the comments you pass, only in files you are already changing.** Delete one the code now says, and cut a three-idea one to its one idea. Read the code first — some carry a constraint the code can't show. Don't open files to hunt, and don't let the cleanup outgrow the change it rides on.

## Working in the repo

- **Fix stale docs as part of the change that made them stale.** A version, a command, a path, a described behavior — correct it in the same change instead of reporting it as a follow-up. Grep the stale value across every scope's `README.md`, `ARCHITECTURE.md`, and `AGENTS.md`, and check open PR branches that rewrite the same doc.
- **A token swap is not a doc fix.** After renaming a symbol in prose, read each changed sentence and fix the claim around it, not just the name.
- **Reuse before designing.** Search production code, dependencies, and platform facilities for the existing owner of a concept. Prefer direct use, extension, or composition; establish the semantic limitation before introducing a parallel mechanism.
- **One source of truth per rule.** Consolidate repeated decisions, defaults, validation, mappings, and transitions in the smallest production owner; make every caller use it. Share the common primitive, not a flag-driven function that selects between unrelated workflows.
- **Establish the contract and ownership.** Trace caller-visible success, failure, cancellation, retry, and shutdown behavior. Separate persisted intent, observed availability, and derived values. Additional state, tasks, locks, or lifecycle hooks need a reachable condition, an owner, a lifetime, and a guarantee existing mechanisms cannot supply.
- **No defensive patches without a diagnosis.** Never add a safety net, retry, or idempotency guard for a bug whose root cause you can't explain. Say what you've ruled out and what you'd look at next instead.
- **Verify before handoff.** Run the applicable typechecks, lint, tests, builds, and formatting checks exposed by the affected scopes. Exercise supported callers and artifacts directly; regression tests must fail against the previous behavior. Provision fixtures, containers, or VMs when needed rather than delegating verification to a reviewer.
- **Report failures as failures.** A check, test, or build that fails is reported and fixed before the work is called done — pre-existing or unrelated failures included. Record executed checks and results in the PR; do not substitute manual-testing instructions or bypass hooks.
- **Keep commits focused.** Commit finished, verified work with conventional messages; squash local fixups before opening the PR. Do not rewrite another contributor's commits or discard unpreserved changes.
- **Uncommitted changes you didn't make are not yours to sort.** Ask whether they belong in the commit before staging anything — don't fold them in, and don't leave them behind silently.

## Gotchas

- **Polyglot repo.** Per-component gotchas live in component-level `AGENTS.md` files — read the relevant one before operating on that component (see Sub-scopes).
- **Verify cross-layer changes in order.** Rust → start-core-ts-bindings → SDK rebuild → web/container-runtime type checks. See [ARCHITECTURE.md](ARCHITECTURE.md#cross-layer-verification). Editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is NOT sufficient — start-core (and the SDK bundle, for container-runtime) must be rebuilt before web/container-runtime will see the change.
- **Ask before destructive `make` recipes.** Image flashing, deploy targets (`update*`, `reflash`, `wormhole*`), and `make clean*` consume hours and disk — confirm with the user first.
- **No git submodules.** `projects/start-wrt/openwrt/` looks like vendored source but is a **disposable, gitignored build workspace** (no git repo inside — think `node_modules/`): `make start-wrt-openwrt-setup` rebuilds it from the sha256-pinned upstream OpenWrt release tarball (`projects/start-wrt/build/openwrt-version`) plus the Start9 delta from `openwrt-patches/` (modified upstream files) + `openwrt-overlay/` (added files). Never keep work inside it — every run rebuilds it; change the patch/overlay dirs instead (see [`projects/start-wrt/AGENTS.md`](projects/start-wrt/AGENTS.md) "OpenWrt tree"). Only start-wrt's full _image_ build needs it — every other product, and start-wrt's own binary build, does not.
- **Stale-path watch.** Old docs referenced `core/`, `web/`, `sdk/`, `container-runtime/`, `patch-db/` at the repo root, and the products + `shared/` directly at the root. Those are gone — products now live under `projects/`, the shared libs under `shared-libs/`; use the locations above.

## Coupled changes (keep in sync)

Some pairs of files mirror each other by hand — nothing enforces them, so a change to one half is incomplete until you update the other. Update both in the **same** commit:

- **A product's CI `paths:` filter ↔ its `build.mk` prerequisites.** Each `.github/workflows/<product>.yaml` only triggers on the paths that product's build actually depends on. Those `paths:` allowlists are a hand-maintained mirror of the prerequisites in `projects/<product>/build.mk` (the project dir, `shared-libs/**` or the specific crates it pulls in, `Cargo.*`, `build/**`, `debian/**`, the web config for products with a UI, …). When you add or drop a build input in a `build.mk`, update that product's workflow `paths:` (both the `push:` and `pull_request:` blocks) — otherwise CI will silently stop running on changes that affect the build. Affected pairs: `start-cli`, `start-registry`, `start-tunnel`, `start-wrt`, `startos-iso`. Additionally, `startos-iso.yaml`'s `changes` job carries a finer mirror: on PRs it gates the expensive **image** matrix on a regex of image-_assembly_ paths (packaging, image-recipe, systemd units, `apt/**`, shared `build/**`) — the inputs the image target pulls in _beyond_ the compiled binary. When you change what feeds the image target in `projects/start-os/build.mk` (vs. the binary, which the `compile` job always covers), update that regex too, or image-affecting PRs will skip image validation.
- **Pull request retargets run through `.github/workflows/pr-retarget.yml`.** A base change is an `edited` event, but trigger-level `branches:` and `paths:` filters evaluate the new base and diff before a workflow starts. The unfiltered listener therefore calls the explicit base-sensitive set: `test`, `start-cli`, `start-registry`, `start-tunnel`, `start-wrt`, and `startos-iso`; title and body edits finish in its classifier without replacing existing check conclusions. Keep each called target reusable through `workflow_call`, give its concurrency group a workflow-specific literal prefix, and leave `edited` out of its direct `pull_request:` types. `conflict-markers` remains a direct unfiltered `pull_request` listener and rescans every edit, because its `Conflict Markers` job must itself replace any stale verdict after a retarget. `live-docs-guard` listens directly too, on `pull_request_target` so a fork's PR is checked without waiting for workflow approval: it keeps its `live-docs` branch filter but takes `edited` too, since the filter sees the new base and a PR retargeted onto `live-docs` must be checked before its next push. It reads the changed paths from the API and must never check out the pull request. Grant called jobs their minimum permissions: `packages: write` belongs only on the `start-registry` call and remains confined inside it to `create-image`; its compile job explicitly downgrades to `contents: read`. `.github/workflows/pr-retarget-conflict.yml` reports GitHub's native conflict verdict through its PR-numbered `Mergeability` job; the `pull_request_target` run itself supplies the pending and final check result without writing a commit status. Base changes use mergeability-specific concurrency and poll until GitHub resolves mergeability; other edits finish under a PR-numbered `Metadata edit` job with separate concurrency. When a base-change event's head or base SHA is stale, it cancels its own run so the obsolete check finishes neutral. It never replaces the separate `Conflict Markers` check, whose real head/base scan clears stale marker failures after a clean retarget. The privileged listener must never check out or execute pull request code. `.github/scripts/check-pr-retarget-workflows.py` enforces independent audited target sets, permissions, concurrency, event routing, and conflict-listener isolation.
- **start-wrt's CI publish constants ↔ `scripts/manage-release.sh`'s wrt config.** The `deploy` job in `.github/workflows/start-wrt.yaml` registers builds into the beta registry with values (registry URL, S3 CDN, platform, compat floor) and register/index commands that hand-mirror `manage-release.sh`'s `STARTWRT_*` vars and `cmd_register` (the manual fallback). Change one side, change the other.
- **`projects/start-os/release-notes/<version>.md` ↔ the StartOS image.** `projects/start-os/build.mk` resolves the file for `VERSION.txt`, packages it as `/usr/lib/startos/release-notes.md`, and makes it an image input without making it a Rust binary input. `version/release_notes.rs` reads the packaged file after an update, so a version bump without its notes file fails the build rather than shipping a server that cannot say what changed. Renaming or moving the source directory requires updating `build.mk` plus `startos-iso.yaml`'s trigger, image-change filter, and deploy `sparse-checkout`.
- **A deploy job's `sparse-checkout` ↔ the paths `manage-release.sh notes` reads.** The `deploy` jobs in [`startos-iso.yaml`](.github/workflows/startos-iso.yaml) and [`start-wrt.yaml`](.github/workflows/start-wrt.yaml) check out only what they need and call the script to compose the registry's release notes, so their checkout must cover `scripts/`, that product's `release-notes/`, and the manifest its version is read from. Teach the script to read something else and widen both, or the register step warns and registers a version with no notes.
- **`startos-iso.yaml`'s `PRUNE_REGISTRIES` ↔ every registry that can reference `s3://startos-images`.** The deploy job's `Prune superseded images` step reclaims an image once it is `MIN_AGE_DAYS` old, its version is still published by some registry in that list, and no registry in that list references its commit — so a registry left out of it is a registry whose images get deleted, for every version another registry still publishes. It is deliberately wider than the job's own `REGISTRY` map — production is in it even though this workflow never deploys there, because `os promote` copies asset URLs through verbatim and production ends up pointing at objects alpha has stopped referencing. Stand up a new channel, or point a registry at this bucket by hand, and it belongs in that list before the next deploy runs.
- **The reusable service-package CI ↔ the SDK package-template ↔ the packaging docs.** `.github/workflows/{build,release,syncNext,tagAndRelease}.yml` (the `workflow_call` CI that external `*-startos` service repos consume) are mirrored by the copies under `projects/start-sdk/docs/package-template/.github/workflows/` and the examples in `projects/start-sdk/docs/src/project-structure.md`. Change the reusable-workflow surface (inputs, action names, file layout) in all three.
- **The docs bot's environment policy ↔ the workflow that uses it.** `master` and `live-docs` are PR-protected with no bypass for `github-actions[bot]`, so [`docs-backport.yml`](.github/workflows/docs-backport.yml) and [`docs-sync-on-tag.yml`](.github/workflows/docs-sync-on-tag.yml) mint a token from a GitHub App that **is** a ruleset bypass actor. Granting an App bypass would be a repo-wide escalation — any workflow on any branch could then write to master — so the App's private key is an **environment** secret, and each environment's deployment policy admits exactly the ref its workflow triggers on (`docs-backport` → the `live-docs` branch; `docs-publish` → `*/v*` tags). **Change a trigger and you must change that environment's policy**, or the job silently loses the secret and every run fails at token minting. Never move those secrets to repo scope, and never add a second workflow to an existing environment without checking its ref policy still describes the whole set.
- **A book's source dir ↔ `docs-deploy.yml`'s `paths:` ↔ `projects/start-docs/versions.conf`.** `versions.conf` alone drives the build, the routing `build.sh` writes into the tree (the unversioned-URL stubs and `404.html`), and llms.txt — but the deploy only _triggers_ on the paths listed in [`.github/workflows/docs-deploy.yml`](.github/workflows/docs-deploy.yml), a hand-maintained mirror of the set of product `docs/` trees. Add a book without adding its path there and edits to it publish nothing.
- **[`apt-publish-alpha.yml`](.github/workflows/apt-publish-alpha.yml)'s `workflow_run.workflows` ↔ the deb products' workflow `name:` fields.** One workflow publishes the whole `alpha` suite, because the three products share a pool and each publish uploads with `--delete-removed` — concurrent writers prune each other, and serializing them is not enough (GitHub keeps only one _pending_ run per concurrency group, so a third publisher supersedes a queued one). It therefore collects each product's latest successful master build rather than the artifacts of the run that triggered it, which is what makes a superseded run harmless. The trigger matches on workflow **name**, not filename: rename a product workflow without updating that list and alpha silently stops receiving that product's packages. Adding a deb product means adding it in both places.
- **`debian/build.sh`'s `Git-Hash` control field ↔ `manage-release.sh`'s `promote_alpha_debs`.** That field is the only thing tying a published `.deb` back to the commit that built it — the pool filename loses the hash (`dpkg-name` rewrites it to `<package>_<version>_<arch>.deb`) and `Version` repeats across every master build. Drop or rename it and deb releases stop being able to verify what they are promoting.
- **`debian/publish.sh`'s suites ↔ `apt/*.list` ↔ the signing key.** Each suite needs a sources.list line for the people who consume it: `stable` is `apt/start9.list` (installed onto StartTunnel hosts by `projects/start-tunnel/build.mk`), `alpha` is `apt/start9-alpha.list` (added by hand by testers). A new suite is a new file. **Each suite is signed independently** — the trust chain is that suite's own `dists/<suite>/InRelease`, so a suite CI publishes must use a CI-only key, and its `.list` line must name the matching `signed-by=` keyring. Never point a CI-published suite at `start9.gpg`: that would mean handing CI the production key.
- **Adding a product or crate.** A new crate must be added to the root `Cargo.toml` `members`; a new _product_ also needs its `projects/<product>/build.mk` `include`d in the root `Makefile`, a path-gated `.github/workflows/<product>.yaml`, and — if it ships a UI — an `angular.json` project plus `package.json` scripts. It also needs an intake path: a GitHub label, a bug form, and an option in both dropdowns (see the next bullet). Blank issues are off, so an issue type without a form is one a human cannot file.
- **The feature and task forms' project dropdowns ↔ `OWNERS` in `issue-triage.yml` ↔ the labels that exist on the repo.** An issue form's `labels:` is a static array, so a dropdown cannot drive it. The eight bug forms sidestep this by being per-product and carrying a static label; [`9-feature-request.yml`](.github/ISSUE_TEMPLATE/9-feature-request.yml) and [`10-task.yml`](.github/ISSUE_TEMPLATE/10-task.yml) span every product, so [`.github/workflows/issue-triage.yml`](.github/workflows/issue-triage.yml) reads their **Project** selection and applies the matching label. The two dropdowns' `options`, the workflow's `OWNERS` keys, and the repo's actual labels must all agree — an option missing from `OWNERS` silently applies no label and gates the issue instead of routing it, and a label renamed on GitHub breaks all of them. The workflow never removes a project label, so a deliberate second one survives an edit.
- **Assignment lives only in that workflow's `OWNERS` map — not on the forms.** A form's `assignees:` fires in the web UI alone, so it would leave every `gh issue create` issue unowned; the forms deliberately carry none. The workflow maps project label → owner, and issue type `Feature` → `FEATURE_OWNER` regardless of project; an issue missing either is gated `needs-triage` and parked on `TRIAGE_OWNER`. It assigns only when the issue has no assignee, so a manual reassignment is never overwritten — with one exception: when the gate lifts and `TRIAGE_OWNER` is the sole assignee, it is swapped for the routed owner, so assigning `TRIAGE_OWNER` by hand to a still-gated issue does not stick. Adding a product means a new `OWNERS` entry, not an `assignees:` block.
- **The server hostname limit ↔ every surface that states or repairs it.** `MAX_LEN` in [`shared-libs/crates/start-core/src/hostname.rs`](shared-libs/crates/start-core/src/hostname.rs) is 32 and governs new input plus migration, restore, and transfer repair. The number is restated by hand in `MAX_LENGTH` in [`shared-libs/ts-modules/shared/src/util/hostname.ts`](shared-libs/ts-modules/shared/src/util/hostname.ts), the `Must be N characters or less` string in all five web dictionaries, `help.arg.hostname` in all five locales of `locales/i18n.yaml`, the `///` docs on `SetupExecuteCliParams::hostname` and `SetServerHostnameParams::hostname`, their generated man pages and TS binding, [`projects/start-os/docs/src/server-name.md`](projects/start-os/docs/src/server-name.md), and — until each release is cut — the pending fragments stating it in `projects/start-os/changelog/` and `projects/start-cli/changelog/`. Move one and you must move all of them, or the form accepts or preserves a name the server rejects.
- **Each bug form's environment fields ↔ its `### Environment` line under [Filing issues](#filing-issues).** Agents never see the forms, so that section restates what each product's form asks for — a hand-maintained mirror, deliberately duplicated because a pointer would not be followed. Add, drop, or rename an environment field on a bug form and you must update its line there in the same change, or agent-filed and human-filed reports of the same bug stop carrying the same facts.

Already enforced or checked elsewhere (listed here for completeness; documented at their own scope):

- **Exported Rust types → `make start-core-ts-bindings` → SDK rebuild → web/container-runtime.** See [ARCHITECTURE.md](ARCHITECTURE.md#cross-layer-verification); editing `osBindings/*.ts` alone is not enough.
- **CLI subcommands and their help strings → `make manpages`.** The committed pages under `projects/*/man/` are generated from the clap definitions in `start-core`; `Generated Artifacts` regenerates both these and the TS bindings and fails on any drift.
- **User-facing strings ↔ all five locale dictionaries** (`en_US`/`de_DE`/`es_ES`/`fr_FR`/`pl_PL`) — compile-checked for `start-core`; `npm run check:i18n` for the web libs.
- **`patchdb-ui-seed.json` ↔ `patchdb-ui-seed.beta.json`** — keep both seeds in sync (see [`projects/start-os/AGENTS.md`](projects/start-os/AGENTS.md)).
- **A product's version bump ↔ its fragments and release notes.** The manifest owns the version; fragments record the impact of the changes being released.
- **Root `package.json` ↔ `projects/start-os/Cargo.toml`'s label ↔ `version::Current`** — the three spellings of the StartOS version. `manage-release.sh pre-check start-os` fails on a stale label, and `version::tests::current_matches_manifest` fails if `Current` drifts from `package.json`.
- **StartOS install/update docs' GitHub release link ↔ the OS release.** `projects/start-os/docs/src/installing-startos.md` and `update-040.md` pin the release URL to the shipping version — a repo-wide `releases/latest` resolves to whichever product released most recently (e.g. StartTunnel), not StartOS. `manage-release.sh pre-check start-os` fails if a doc still links to `releases/latest` or pins a stale version, so bump these links with the release.
- **The package template's SDK pin ↔ the prospective SDK version.** `projects/start-sdk/docs/package-template/package.json` pins the `@start9labs/start-sdk` version scaffolded packages build against. Bump it with `make -C projects/start-sdk sync-template` in the same change as `projects/start-sdk/package.json`; packaging workspaces track `live-docs`, so changing `master` does not update their template. The SDK tag advances it. `manage-release.sh pre-check start-sdk` fails if it pins a different version than the release being cut, or if the template commits a `package-lock.json` (a generated artifact that only rots against the pin).

## Sub-scopes

- [`projects/start-os/AGENTS.md`](projects/start-os/AGENTS.md) — OS product
- [`projects/start-os/container-runtime/AGENTS.md`](projects/start-os/container-runtime/AGENTS.md) — Node.js LXC service runtime
- [`projects/start-cli/AGENTS.md`](projects/start-cli/AGENTS.md) — CLI wrapper over `start-core`
- [`projects/start-registry/AGENTS.md`](projects/start-registry/AGENTS.md) — registry server wrapper
- [`projects/start-tunnel/AGENTS.md`](projects/start-tunnel/AGENTS.md) — tunnel server + UI
- [`projects/start-wrt/AGENTS.md`](projects/start-wrt/AGENTS.md) — OpenWrt-based router OS (Rust backend + Angular UI in the root workspace + pinned-upstream OpenWrt image build)
- [`projects/start-sdk/AGENTS.md`](projects/start-sdk/AGENTS.md) — TypeScript service-packaging SDK, plus the packaging mdbook in `docs/`
- [`projects/brochure-marketplace/AGENTS.md`](projects/brochure-marketplace/AGENTS.md) — public marketplace site
- [`projects/start-docs/AGENTS.md`](projects/start-docs/AGENTS.md) — documentation website; also the authoring conventions for every product book
- [`shared-libs/AGENTS.md`](shared-libs/AGENTS.md) — shared libs container: [`crates/start-core`](shared-libs/crates/start-core/AGENTS.md) (Rust backend), [`ts-modules`](shared-libs/ts-modules/AGENTS.md) (Angular workspace + UI/setup-wizard/shared libs)
- `shared-libs/crates/patch-db/` — first-party crate (maintained in-tree)
