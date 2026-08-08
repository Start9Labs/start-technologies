# AGENTS.md

Agent/developer operating rules for the **start-technologies monorepo root**. This repo is the monorepo for all Start9 products. `CLAUDE.md` is a one-line `@AGENTS.md` import — do not edit it.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the layout and [CONTRIBUTING.md](CONTRIBUTING.md) for the build/test/format workflow.

**Keep these docs current.** Every scope carries `AGENTS.md` / `ARCHITECTURE.md` / `README.md`, most still with a `CONTRIBUTING.md` beside them (and `CLAUDE.md`, a one-line `@AGENTS.md` import). When a change alters structure, conventions, the build/test/release flow, or product context, update the matching doc(s) in the **same change** — never defer. These docs are **hierarchical**: each scope's docs cover only what is specific to it and must not repeat anything already stated at a higher scope (e.g. commit/PR conventions live only in this root `CONTRIBUTING.md`).

**Anything an agent must follow belongs _in_ `AGENTS.md`, not behind a link from it.** The `AGENTS.md` standard is plain Markdown with **no import syntax** — unlike `CLAUDE.md`, whose `@file` imports expand into context at launch. So a pointer in `AGENTS.md` ("see X for the release process") is just a suggestion that the agent spend a tool call, and agents routinely don't: that is exactly how start-sdk 2.0.4 and 2.0.5 shipped untagged. Inline the rule; link only to _reference_ material an agent can safely skip (`ARCHITECTURE.md`, a product book). `CLAUDE.md` stays a one-line `@AGENTS.md` import — Claude Code does not read `AGENTS.md` natively.

**So `CONTRIBUTING.md` is being folded into `AGENTS.md` — in every scope _but_ the root.** A sub-scope `CONTRIBUTING.md` earns nothing (GitHub gives it no special treatment) and costs an agent a hop, so its contents belong in that scope's `AGENTS.md`. `projects/start-sdk/` and `shared-libs/crates/start-core/` are migrated and have none; every other scope still has the split. When you next do substantial work in a scope that hasn't migrated, fold its `CONTRIBUTING.md` into its `AGENTS.md`, delete it, and repoint every inbound link in the same change — don't leave dangling cross-references, and don't migrate scopes you aren't otherwise touching. **The root [`CONTRIBUTING.md`](CONTRIBUTING.md) stays**: GitHub surfaces it (the contributing prompt on new issues/PRs, the community profile), so it remains the human-facing front door — the social layer, and a pointer _to_ `AGENTS.md` for the mechanics. That pointer runs one way; a human will follow a link, an agent won't.

**A product's user docs and changelog ship with the code.** Any change that alters user-visible behavior **must** update that product's user-facing documentation book (its `docs/` directory — e.g. `projects/start-os/docs/`, `projects/start-tunnel/docs/`, `projects/start-sdk/docs/`) in the **same change**, and **must** add a `CHANGELOG.md` entry for that product (a version bump always pairs with its changelog). Don't land code and defer its docs or changelog to a follow-up. The conventions for authoring **any** of those books — mdBook versions, admonitions, tabs, `SUMMARY.md`, the shared `theme/` — live in [`projects/start-docs/AGENTS.md`](projects/start-docs/AGENTS.md) and its `CONTRIBUTING.md`. That project is a sibling, not an ancestor, so nothing loads it for you: read it before editing book pages anywhere in the repo.

**The changelog's top heading is the prospective _next_ version, and git tags decide released-vs-unreleased.** **Before deciding where a changelog entry goes, freshly pull tags from origin _first_** — run `git fetch --tags origin` (or query origin live: `git ls-remote --tags origin '<product>/v*'`, `gh release list`), every time; never trust stale local tags, and never infer release state from the changelog file, a `## [x.y.z]` heading, or a manifest constant. Those origin tags (`<product>/v<version>`) are the only source of truth for what has shipped. Keep the top `CHANGELOG.md` heading set to the **actual prospective next version** (e.g. `## [1.1.1]`) matching the product manifest — not a bare `## [Unreleased]` — because the number itself signals the tier of change accumulated (patch/minor/major). When that top version has **no matching origin tag it is unreleased: add your entry _under_ it** (in the right `### Added`/`### Changed`/`### Fixed`/`### Security` subsection), and raise both the heading and the manifest a tier only if your change warrants it (a fix leaves an accumulating `1.1.1` alone; a breaking change bumps it to `2.0.0`). **If your change fixes or refines a feature that was _added in that same still-unreleased version_, edit that feature's existing entry** (only where its wording needs it) rather than adding a separate `### Fixed` line — to the user the feature simply ships correct, so there is no fix to a thing they never received. **Only cut a _new_ heading — and bump the manifest — once the current top heading is a cut origin tag** (that line is fully released). The release tooling turns that prospective heading into the shipped one when it cuts the tag.

**Read [CONTRIBUTING.md](CONTRIBUTING.md) before making _any_ code changes.** It carries the build/test/format workflow and the commit/PR conventions every change must follow — read it first, before you touch code. This is hierarchical like `AGENTS.md`: if a subdirectory you touch carries its own `CONTRIBUTING.md`, read that one too — and any further nested `CONTRIBUTING.md` on the way down to the files you're editing — before changing anything there.

**Read down into what you touch.** When you work in a subdirectory, first read its `AGENTS.md` — and any further nested `AGENTS.md` on the way down to the files you're editing — before changing anything. Each scope's docs assume you've read the scopes above it, so a subdir's `AGENTS.md` adds only its own rules on top of this root.

## Layout

Each product lives under `projects/` as a thin wrapper; the bulk of the code lives in the top-level shared libs (`shared-libs/`).

- `projects/start-os/` — OS product. Rust bins `startbox` + `start-container` (`src/bin/*.rs`), `web/` (Angular UI + setup-wizard), `container-runtime/` (Node LXC service runtime), bin build scripts + OS image build (`build/`), Debian control (`debian/`), VM-setup `assets/`, `backup-fs/`, `docs/`, `*.service`.
- `projects/start-cli/` — `start-cli` bin (`src/main.rs`); thin wrapper over `start-core`.
- `projects/start-registry/` — `registrybox` bin; registry server, serves the shared marketplace UI lib.
- `projects/start-tunnel/` — `tunnelbox` bin + `web/` (StartTunnel UI).
- `projects/start-wrt/` — StartWRT, an OpenWrt-based router OS. Rust backend (`startwrt` bin: RPC daemon + CLI, crates `ctrl`/`uciedit`/`uciedit_macros`) building on shared `start-core`; an Angular `web/` UI (a project in the root Angular workspace) embedded into the binary; a build-managed `openwrt/` workspace (pinned upstream OpenWrt tarball + the Start9 delta in `openwrt-patches/` + `openwrt-overlay/`); flashable image for the SpaceMiT K1.
- `projects/start-sdk/` — `@start9labs/start-sdk` (source in `lib/`; imports the shared `@start9labs/start-core` lib and bundles it into its published `dist/`) + `Makefile`/`s9pk.mk` + `docs/` (packaging mdbook).
- `projects/brochure-marketplace/` — public marketplace/landing Angular app (deploys to marketplace.start9.com).
- `projects/start-docs/` — the documentation website (build infra + landing + Bitcoin guides; each product's own book lives in its `docs/`).
- `shared-libs/crates/start-core/` — the **entire** Rust backend lib (package `start-core`, lib name `start_core`). All six bins depend on it. Internally unchanged from the old `core/` crate.
- `shared-libs/ts-modules/` — shared **TypeScript** modules (the common thread is just that they are TS — not Angular-specific). These are the Angular libs `shared/` (`@start9labs/shared`) and `marketplace/` (`@start9labs/marketplace`), plus the non-Angular `start-core/` (`@start9labs/start-core`: SDK core types/ABI/effects/OS bindings, the TS projection of the `start-core` crate, consumed by web and bundled into the SDK; versionless, not published separately). The Angular workspace is rooted at the repo root (`angular.json`/`package.json`). Product apps reference the libs by package name.
- Top level also holds the shared build infra (`build/`, `Makefile`), `apt/`, the shared `debian/build.sh`, `scripts/` (maintainer release tooling — `manage-release.sh <subcommand> <project>` drives a product through pre-check/tag/release/sign/publish; `deploy-migration-payload.sh` publishes a 0.3.5.1 → 0.4.0 migration OTA payload to a legacy registry), `rfcs/` (protocol drafts), and `shared-libs/crates/patch-db/` (first-party crate, consumed by `start-core` and web).

## Build & test (run from the repo root)

- **Use `make` recipes when they exist** rather than re-deriving the underlying commands. The root `Makefile` is a thin orchestrator that `include`s `build/common.mk` (shared vars/macros) and one `<project>/build.mk` per product (`projects/<name>/build.mk`, `shared-libs/*/build.mk`) — run everything from the repo root (`make start-os`, `make start-registry`, etc.); a product's targets live in its `build.mk`. There is no default target — bare `make` prints `help`.
- **Build a single product** with `cargo build -p <crate> --bin <bin>` (bins: `startbox`/`start-container` in package `start-os`; `start-cli`; `registrybox` in `start-registry`; `tunnelbox` in `start-tunnel`; `startwrt` in package `startwrt-core` for `start-wrt`).
- **Tests:** `make test` (all), `make start-core-test` / `make start-sdk-test` / `make container-runtime-test` (scoped). A single Rust test: `cd shared-libs/crates/start-core && cargo test <test_name> --features=test`.
- **Format:** `make format` (rustfmt in a pinned-nightly container + prettier + taplo, both native); CI runs `make format-check`. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full build/test/format workflow.

## Releases

- **Cut every release with [`scripts/manage-release.sh`](scripts/manage-release.sh)** — `./scripts/manage-release.sh release <project>` (`start-os`, `start-cli`, `start-tunnel`, `start-registry`, `start-sdk`, `start-wrt`); `--help` lists the individual subcommands and env vars. A product's version is read from its manifest (`Cargo.toml`, or `package.json` for the SDK), and its git tag / GitHub release is `<project>/v<version>`. **StartOS is the exception:** its version carries a revision segment SemVer cannot express (`0.4.0.1`), so the **root `package.json`** holds it and `projects/start-os/Cargo.toml` carries only a `0.4.0-rev.1` label — never a comparand, since a SemVer prerelease sorts _below_ its release. Read it via `build/env/version.sh`; see [`shared-libs/crates/start-core/VERSION_BUMP.md`](shared-libs/crates/start-core/VERSION_BUMP.md).
- **Never invoke a product's publish step directly** (`make publish`, an upload, a registry index). The pipelines differ per product — npm for the SDK, apt + GitHub release for the debs, S3 + registry promotion for the OS and StartWRT — but all of them run the **idempotent steps (tag, GitHub release) _before_ the irreversible one**. Skip the pipeline and you strand a released version with no tag and no GitHub release, which for npm cannot be undone (`pre-check` then refuses the version and npm won't republish it). This is exactly how start-sdk 2.0.4 and 2.0.5 shipped, and they had to be backfilled. Reach for individual subcommands only to repair a partial release.
- **Release from a merged, up-to-date `master`.** The tag is a claim that a commit on `master` produced the artifact, so cut it where that's true. Nothing enforces this — publishing out of band from an unmerged branch is deliberately still possible, and sometimes the right call — but it is a **debt, not a shortcut**: the commit you published from will be squashed or orphaned when the branch merges, leaving the tag nowhere honest to point. If you take it, you owe the follow-up in the same sitting — merge the branch, then tag and release at the resulting `master` commit, having checked that its shipped subtree still matches the artifact you published. start-sdk 2.0.5 went out this way and had to be reconstructed after the fact.
- Per-product prerequisites and specifics live in that product's scope — e.g. [`projects/start-sdk/AGENTS.md`](projects/start-sdk/AGENTS.md#cutting-a-release), [`projects/start-wrt/CONTRIBUTING.md`](projects/start-wrt/CONTRIBUTING.md#cutting-a-release).

## Filing issues

- **The issue forms are the human path, and you will never see them.** `.github/ISSUE_TEMPLATE/*.yml` binds the web UI only — `gh issue create` does not consult a template, so nothing sets the type or the label on your behalf. Pass both yourself (`gh issue create --type Bug --label StartOS`) and write the body from the spec below; don't go open a form to find out what it wants. An issue filed without a type and label is untyped and unlabeled, and drops out of every triage view.
- **Type is a GitHub issue type, not a label.** `Bug`, `Feature`, or `Task`, defined at the org level and passed as `--type`. There is no `bug` or `enhancement` label — don't invent one.
- **Take the project label from this exact set.** The casing is inconsistent and is matched literally: `StartOS`, `start-cli`, `StartSDK`, `StartWRT`, `StartTunnel`, `start-registry`, `start-docs`, `brochure`, and `repo` (build, CI, and release tooling). One label is the norm — reach for a second only when a defect genuinely spans two products. The shared libraries — `start-core`, `patch-db`, `exver`, `ts-modules` — have none of their own, so label them with the product the defect is most visible in.
- **The two status labels are the maintainer's, not the filer's.** `Approved` means signed off and ready for a PR; `Known Solution` means the fix is identified but unimplemented. Never apply either when filing.
- **Don't pass `--assignee`.** [`.github/workflows/issue-triage.yml`](.github/workflows/issue-triage.yml) routes the issue to its owner from the project label you set, and features to the maintainer regardless of project. Getting the label right is therefore what gets the issue in front of the right person — an unlabeled issue is also an unassigned one.
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

## Code style

- **Comment only what the code can't say for itself.** Add a comment for a non-obvious mechanism, a deviation from convention, or a load-bearing subtlety — not to restate what the code plainly does. Keep it terse: say what needs saying and no more, and prefer cutting a comment to padding it.

## Gotchas

- **Polyglot repo.** Per-component gotchas live in component-level `AGENTS.md` files — read the relevant one before operating on that component (see Sub-scopes).
- **Verify cross-layer changes in order.** Rust → start-core-ts-bindings → SDK rebuild → web/container-runtime type checks. See [ARCHITECTURE.md](ARCHITECTURE.md#cross-layer-verification). Editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is NOT sufficient — start-core (and the SDK bundle, for container-runtime) must be rebuilt before web/container-runtime will see the change.
- **Ask before destructive `make` recipes.** Image flashing, deploy targets (`update*`, `reflash`, `wormhole*`), and `make clean*` consume hours and disk — confirm with the user first.
- **No git submodules.** `projects/start-wrt/openwrt/` looks like vendored source but is a **disposable, gitignored build workspace** (no git repo inside — think `node_modules/`): `make start-wrt-openwrt-setup` rebuilds it from the sha256-pinned upstream OpenWrt release tarball (`projects/start-wrt/build/openwrt-version`) plus the Start9 delta from `openwrt-patches/` (modified upstream files) + `openwrt-overlay/` (added files). Never keep work inside it — every run rebuilds it; change the patch/overlay dirs instead (see [`projects/start-wrt/CONTRIBUTING.md`](projects/start-wrt/CONTRIBUTING.md) "OpenWrt tree"). Only start-wrt's full _image_ build needs it — every other product, and start-wrt's own binary build, does not.
- **Stale-path watch.** Old docs referenced `core/`, `web/`, `sdk/`, `container-runtime/`, `patch-db/` at the repo root, and the products + `shared/` directly at the root. Those are gone — products now live under `projects/`, the shared libs under `shared-libs/`; use the locations above.

## Coupled changes (keep in sync)

Some pairs of files mirror each other by hand — nothing enforces them, so a change to one half is incomplete until you update the other. Update both in the **same** commit:

- **A product's CI `paths:` filter ↔ its `build.mk` prerequisites.** Each `.github/workflows/<product>.yaml` only triggers on the paths that product's build actually depends on. Those `paths:` allowlists are a hand-maintained mirror of the prerequisites in `projects/<product>/build.mk` (the project dir, `shared-libs/**` or the specific crates it pulls in, `Cargo.*`, `build/**`, `debian/**`, the web config for products with a UI, …). When you add or drop a build input in a `build.mk`, update that product's workflow `paths:` (both the `push:` and `pull_request:` blocks) — otherwise CI will silently stop running on changes that affect the build. Affected pairs: `start-cli`, `start-registry`, `start-tunnel`, `start-wrt`, `startos-iso`. Additionally, `startos-iso.yaml`'s `changes` job carries a finer mirror: on PRs it gates the expensive **image** matrix on a regex of image-_assembly_ paths (packaging, image-recipe, systemd units, `apt/**`, shared `build/**`) — the inputs the image target pulls in _beyond_ the compiled binary. When you change what feeds the image target in `projects/start-os/build.mk` (vs. the binary, which the `compile` job always covers), update that regex too, or image-affecting PRs will skip image validation.
- **start-wrt's CI publish constants ↔ `scripts/manage-release.sh`'s wrt config.** The `deploy` job in `.github/workflows/start-wrt.yaml` registers builds into the beta registry with values (registry URL, S3 CDN, platform, compat floor) and register/index commands that hand-mirror `manage-release.sh`'s `STARTWRT_*` vars and `cmd_register` (the manual fallback). Change one side, change the other.
- **The reusable service-package CI ↔ the SDK package-template ↔ the packaging docs.** `.github/workflows/{build,release,tagAndRelease}.yml` (the `workflow_call` CI that external `*-startos` service repos consume) are mirrored by the copies under `projects/start-sdk/docs/package-template/.github/workflows/` and the examples in `projects/start-sdk/docs/src/project-structure.md`. Change the reusable-workflow surface (inputs, action names, file layout) in all three.
- **Adding a product or crate.** A new crate must be added to the root `Cargo.toml` `members`; a new _product_ also needs its `projects/<product>/build.mk` `include`d in the root `Makefile`, a path-gated `.github/workflows/<product>.yaml`, and — if it ships a UI — an `angular.json` project plus `package.json` scripts. It also needs an intake path: a GitHub label, a bug form, and a dropdown option (see the next bullet).
- **The feature form's project dropdown ↔ `OWNERS` in `issue-triage.yml` ↔ the labels that exist on the repo.** An issue form's `labels:` is a static array, so a dropdown cannot drive it. The eight bug forms sidestep this by being per-product and carrying a static label; [`9-feature-request.yml`](.github/ISSUE_TEMPLATE/9-feature-request.yml) is the one form that can't, so [`.github/workflows/issue-triage.yml`](.github/workflows/issue-triage.yml) reads its **Project** selection and applies the matching label. The dropdown's `options`, the workflow's `OWNERS` keys, and the repo's actual labels must all agree — an option missing from `OWNERS` silently applies nothing and leaves the issue unassigned, and a label renamed on GitHub breaks both. The workflow only ever adds, never removes, so a deliberate second project label survives an edit.
- **Assignment lives only in that workflow's `OWNERS` map — not on the forms.** A form's `assignees:` fires in the web UI alone, so it would leave every `gh issue create` issue unowned; the forms deliberately carry none. The workflow maps project label → owner, and issue type `Feature` → `FEATURE_OWNER` regardless of project. It assigns only when the issue has no assignee, so a manual reassignment is never overwritten. Adding a product means a new `OWNERS` entry, not an `assignees:` block.
- **Each bug form's environment fields ↔ its `### Environment` line under [Filing issues](#filing-issues).** Agents never see the forms, so that section restates what each product's form asks for — a hand-maintained mirror, deliberately duplicated because a pointer would not be followed. Add, drop, or rename an environment field on a bug form and you must update its line there in the same change, or agent-filed and human-filed reports of the same bug stop carrying the same facts.

Already enforced or checked elsewhere (listed here for completeness; documented at their own scope):

- **Exported Rust types → `make start-core-ts-bindings` → SDK rebuild → web/container-runtime.** See [ARCHITECTURE.md](ARCHITECTURE.md#cross-layer-verification); editing `osBindings/*.ts` alone is not enough.
- **User-facing strings ↔ all five locale dictionaries** (`en_US`/`de_DE`/`es_ES`/`fr_FR`/`pl_PL`) — compile-checked for `start-core`; `npm run check:i18n` for the web libs.
- **`patchdb-ui-seed.json` ↔ `patchdb-ui-seed.beta.json`** — keep both seeds in sync (see [`projects/start-os/AGENTS.md`](projects/start-os/AGENTS.md)).
- **A crate's `version` bump ↔ its `CHANGELOG.md`** — versions are read from each manifest; bump the changelog in the same change.
- **Root `package.json` ↔ `projects/start-os/Cargo.toml`'s label ↔ `version::Current`** — the three spellings of the StartOS version. `manage-release.sh pre-check start-os` fails on a stale label, and `version::tests::current_matches_manifest` fails if `Current` drifts from `package.json`.
- **StartOS install/update docs' GitHub release link ↔ the OS release.** `projects/start-os/docs/src/installing-startos.md` and `update-040.md` pin the release URL to the shipping version — a repo-wide `releases/latest` resolves to whichever product released most recently (e.g. StartTunnel), not StartOS. `manage-release.sh pre-check start-os` fails if a doc still links to `releases/latest` or pins a stale version, so bump these links with the release like the changelog.
- **The package template's SDK pin ↔ the SDK version being released.** `projects/start-sdk/docs/package-template/package.json` pins the `@start9labs/start-sdk` version scaffolded packages build against. `manage-release.sh pre-check start-sdk` fails if it pins a different version than the release being cut, or if the template commits a `package-lock.json` (a generated artifact that only rots against the pin); run `make -C projects/start-sdk sync-template` to bump it with the release.
- **User-facing changes ↔ that product's `docs/`** — docs are part of the change (see each product's AGENTS/CONTRIBUTING).

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
- [`shared-libs/AGENTS.md`](shared-libs/AGENTS.md) — shared libs container: [`crates/start-core`](shared-libs/crates/start-core/AGENTS.md) (Rust backend), [`web`](shared-libs/ts-modules/AGENTS.md) (Angular workspace + UI/setup-wizard/shared libs)
- `shared-libs/crates/patch-db/` — first-party crate (maintained in-tree; the standalone `Start9Labs/patch-db` repo is being retired)
