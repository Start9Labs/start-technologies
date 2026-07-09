# StartOS Packaging — Agent Context

> [!NOTE]
> This page is the `AGENTS.md` that `start-cli s9pk init-workspace` links into every packaging workspace. Your workspace copy is a symlink to this file, so syncing the guide keeps it current.

You are an AI assistant working in a **StartOS packaging workspace**. You help create, maintain, and update `.s9pk` service packages for StartOS. This file is your always-on context: the rules to follow, the patterns to know, and a map of where to read for any given task. The substance lives in the packaging guide under `start-technologies/projects/start-sdk/docs/` — read those pages locally, on demand, as the task requires. Do not load everything at once.

## Workspace layout

```
<workspace>/
├── .startos/              ← workspace marker: build-key (signs your packages) + config.yaml (hosts, registries)
├── AGENTS.md              ← this file (symlink → start-technologies/projects/start-sdk/docs/src/agent-context.md)
├── AGENTS.local.md        ← your workspace-specific notes (never overwritten by a sync)
├── CLAUDE.md              ← loads AGENTS.md + AGENTS.local.md (Claude Code bridge)
├── start-technologies/    ← checkout of the Start9 monorepo: the packaging guide, plus the SDK and OS source
└── <id>-startos/ …        ← one or more package repos
```

Each package repo holds: `README.md` (what it is / how it differs from upstream), `instructions.md` (end-user docs shown in StartOS), `UPDATING.md` (upstream-version tracking), `TODO.md` (pending work), and `startos/` (the SDK code).

**The workspace root is not a git repository** — each package is its own repo, and commits, diffs, and pushes happen inside them. Files at the workspace root (`AGENTS.local.md`, `.startos/`, scripts of your own) are untracked; don't run `git status` against the root or try to fold a root-level change into a package's commit.

## Keeping the workspace current

The guide, the package template, and this file all live in `start-technologies/`, so syncing it refreshes everything at once. **At the start of every session, fast-forward it against its remote:**

```
git -C start-technologies pull --ff-only
```

To track a different source (e.g. a fork), repoint `start-technologies`'s remote first — the sync follows whatever remote is configured.

Keep workspace-specific notes in `AGENTS.local.md`; a sync never touches it. That file is for what is true of _your_ setup — your box, your registry, your packages, any departure from the scaffolded layout. Anything that would help **every** packager belongs in the guide instead: open a PR against `start-technologies` rather than letting it drift in one workspace.

If `start-technologies/` is a **symlink** to a checkout maintained outside this workspace, skip the sync: that repo has its own branches and its own work in progress, so its state is the owner's to manage, not this workspace's.

## How to use the guide (local-first)

The guide has two layers:

- **Recipes** — intent-driven pages: _what_ to do and _which_ constructs to combine. **Start here.** Each recipe names the SDK APIs and files involved and links to the reference pages and to real packages.
- **Reference** — concept pages documenting each SDK construct in depth with code examples.

Workflow for any task:

1. **Find the recipe.** Open the intent index: `start-technologies/projects/start-sdk/docs/src/recipes.md`.
2. **Follow its reference links** for API details and code examples.
3. **Follow its package links** — read the specific files it names in a real package (`startos/main.ts`, `startos/actions/`, …) for working production code.
4. **Read only what the task needs.**

Read pages from your local checkout (`start-technologies/projects/start-sdk/docs/src/<page>.md`). Only if `start-technologies/` is missing, fall back to the web (`https://docs.start9.com/packaging/<page>.html`).

## Where to read for X

| Need                                                | Read                                                                     |
| --------------------------------------------------- | ------------------------------------------------------------------------ |
| Find the right recipe for a task                    | `start-technologies/projects/start-sdk/docs/src/recipes.md`              |
| How to behave on every task (the disciplines below) | `start-technologies/projects/start-sdk/docs/src/workflow.md`             |
| File/directory layout of a package                  | `start-technologies/projects/start-sdk/docs/src/project-structure.md`    |
| Service metadata, descriptions                      | `start-technologies/projects/start-sdk/docs/src/manifest.md`             |
| Versions, migrations, release notes                 | `start-technologies/projects/start-sdk/docs/src/versions.md`             |
| Daemons, health checks, oneshots, lifecycle         | `start-technologies/projects/start-sdk/docs/src/main.md`                 |
| Install / update / restore init logic               | `start-technologies/projects/start-sdk/docs/src/init.md`                 |
| Network interfaces and ports                        | `start-technologies/projects/start-sdk/docs/src/interfaces.md`           |
| User-facing actions                                 | `start-technologies/projects/start-sdk/docs/src/actions.md`              |
| Prompting the user to run actions                   | `start-technologies/projects/start-sdk/docs/src/tasks.md`                |
| Config files as typed models                        | `start-technologies/projects/start-sdk/docs/src/file-models.md`          |
| Service dependencies                                | `start-technologies/projects/start-sdk/docs/src/dependencies.md`         |
| Build / install commands                            | `start-technologies/projects/start-sdk/docs/src/makefile.md`             |
| Writing the README                                  | `start-technologies/projects/start-sdk/docs/src/writing-readmes.md`      |
| Writing user instructions                           | `start-technologies/projects/start-sdk/docs/src/writing-instructions.md` |
| Publishing / registries                             | `start-technologies/projects/start-sdk/docs/src/publishing.md`           |
| `start-cli` reference                               | `start-technologies/projects/start-sdk/docs/src/cli.md`                  |

## Reading the SDK and OS source (last resort)

`start-technologies/` is a checkout of the whole Start9 monorepo, so beyond the guide you already have the **SDK source** (`start-technologies/projects/start-sdk/lib`) and the **StartOS source** (`start-technologies/projects/start-os`, plus the shared core in `start-technologies/shared-libs/`) on disk. Nothing to fetch.

Reach for them **only when the recipes, reference pages, real packages, and the installed SDK types (`node_modules/@start9labs/start-sdk`) don't answer the question** — e.g. to confirm exactly what an SDK call does, or how an OS effect behaves. Open one file to settle one question; don't browse the monorepo to "understand the system."

If what you find there is a bug, say so. You are standing in a git repo you can branch from and open a pull request against.

## Key patterns

Understand these before writing any code (full detail on the pages above):

- **Daemons** define the containers that run your service — subcontainer, exec command, `ready` health check, and a `requires` array for startup ordering. (`main.md`)
- **Oneshots** run a command to completion before dependent daemons start — file ownership (`chown`), migrations, wallet unlocks, config generation. Chained with `.addOneshot()` alongside `.addDaemon()` in `setupMain()`. (`recipe-oneshot.md`, `main.md`)
- **Health checks** come in two forms: the `ready` property on every daemon, and standalone `.addHealthCheck()` calls for ongoing conditions (sync progress, reachability). (`main.md`)
- **runUntilSuccess** spins up a temporary daemon chain during install to bootstrap a service through its own API, then tears it down. (`recipe-run-until-success.md`)
- **File models** are zod-typed representations of config files (JSON, YAML, TOML, …) providing defaults, validation, and reactive reads — the backbone of configuration. `merge(effects, {})` fills missing fields from their `.catch()` defaults and repairs invalid ones; it never strips a key you didn't name. It is not a way to clean or regenerate a config. (`file-models.md`)
- **Interfaces** declare what your service exposes; the **user** decides where it's reachable. `type` (`'ui'`/`'api'`/`'p2p'`) is a label, not a control, and Tor is a service the user installs and enables per interface. Never claim a service is on Tor or the public internet. (`interfaces.md`)

## Golden rules

- **Start from intent, not from API.** Find the recipe before diving into reference pages.
- **Code lives in reference pages and packages, not recipes.** Recipes describe the pattern; reference pages have the API; real packages have production implementations.
- **Match existing patterns — but a neighbouring package is not the authority.** Read a package's code before introducing a new pattern. Then check it against the recipe: the fleet is mid-migration, so the package you happened to grep may itself be non-conformant. "It matches the package next door" is not a quality bar. A recipe and its named reference implementation outrank a package you found by searching.

## Working discipline (every task)

The full rules are in `start-technologies/projects/start-sdk/docs/src/workflow.md`; this is the digest.

- **Verify facts; don't assert from memory.** Image names, tags, version numbers, config formats, credential schemes — confirm each with a tool before you rely on it. "I know that X" is a cue to check X, not to write it down. Guessing an image that doesn't exist or a password format the app rejects fails silently.
- **A comment is not evidence.** A comment claiming what an SDK call does — in a package, in a review, in this guide — is a claim to check against the reference page, the installed types, or the SDK source. Don't accept or repeat it unverified; wrong semantics propagate from package to package.
- **Compiling is not working.** A green `tsc` and a clean `s9pk pack` prove the code builds, not that the service runs. Before reporting a feature done, exercise it against a running service (install, log in, write data, restart). State what you verified and what you didn't — never imply a feature works when you only compiled it.
- **Don't fabricate; verify or flag.** Never ship an invented icon/logo, a config format you didn't confirm, or placeholder facts in the README. Fetch the real thing, or leave it and flag the gap in `TODO.md`.
- **Search before declaring impossible.** Before working around a limitation, grep the SDK types (`node_modules/@start9labs/start-sdk/**/*.d.ts`) and existing packages. "The SDK can't do X" is a claim to verify in the types, not a conclusion from the docs (this is how `runAsInit` is found).
- **Keep `README.md` and `instructions.md` in sync.** `README.md` tracks architecture/behavior (for developers and AI); `instructions.md` tracks user-visible changes — update each in the same change as the code. Content rules: `writing-readmes.md`, `writing-instructions.md`.
- **Iterate with a dirty tree; commit once.** The `-modified` pack-hash suffix is informational — don't commit between test attempts. One clean commit when the package works; `git reset --soft HEAD~N` collapses accumulated fixups.
- **Pre-existing errors are still errors.** A red `tsc`, test, or pack step means the package doesn't pass, even if unrelated to your change. Fix it or flag it; never report green when a check was red.
- **Don't create unnecessary version files.** The latest version always lives in `startos/versions/current.ts`; most bumps just edit that file in place. A new file is spun off only when the bump carries a migration. See `versions.md` (When to Create a New Version File, Release Notes).

## Starting a new package

**Scaffold first — run `start-cli s9pk init-package "<Name>"`. Do not hand-assemble a package by copying files out of another one.** Scaffolding produces a barebones hello-world clone with a `TODO.md` checklist. **Then work `TODO.md` top to bottom** — it takes the package from clone to release-ready (descriptions, image, icon, interfaces, daemons, docs, first build, install-and-verify). Keep it as the live worklist: remove items as you complete them, add items when you defer work. Wrapping an existing upstream Docker image? Read `recipe-prebuilt-image.md` first.
