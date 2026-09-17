---
name: package-service
description: Package an open-source service for StartOS as a brand-new .s9pk, working inside a StartOS packaging workspace. Researches the upstream project and how people actually self-host it, settles the package's shape with the user in one round of questions, then scaffolds, builds, and verifies it on a StartOS box without further input. Use when asked to package, wrap, or "make a StartOS service out of" a named project or an upstream URL.
license: MIT
---

# Package a service for StartOS

The argument is a service name or an upstream URL. Take it from nothing to a working,
verified, **uncommitted** package.

Paths below are relative to the **packaging workspace root** — the directory holding
`.startos/`, `AGENTS.md`, and the `start-technologies/` guide checkout. Phase 0 finds it.

| Phase                       | Who works                                       |
| --------------------------- | ----------------------------------------------- |
| 0 · Workspace and prior art | you, asking only if a workspace has to be built |
| 1 · Research the upstream   | you                                             |
| 2 · The gate                | the user answers, once                          |
| 3 · Package it              | you, alone                                      |
| 4 · Verify on a StartOS box | you, alone                                      |
| 5 · Hand back               | the user reviews                                |

**The gate is the only interruption in the packaging work.** Everything before it exists
to make it a good one; everything after it runs to completion. Judgement calls inside
phases 3 and 4 are yours — make them, state them at hand-back, and let the user push back
there.

Phase 0 may ask one thing first: permission to build a workspace or install a missing
prerequisite. That is consent for an action on the machine, not a packaging decision, and
it is asked before any research begins.

## Read first

Nothing loads these for you from inside a package directory, and each outranks your
priors:

| Read                                                                      | For                                                                                                                                                    |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `AGENTS.md`                                                               | the packaging guide's map — which page answers which objective                                                                                         |
| `AGENTS.local.md`                                                         | this workspace's own rules: where packages live, which box to install to, how a finished package lands. **Where it departs from this skill, it wins.** |
| `start-technologies/projects/start-sdk/docs/src/recipes.md`               | the intent index — find a recipe for every part of the shape you agreed                                                                                |
| `start-technologies/projects/start-sdk/docs/src/new-package-checklist.md` | the build order once scaffolded                                                                                                                        |

## Phase 0 — Workspace and prior art

Nothing else runs without a workspace: `make`, `s9pk pack`, `s9pk publish` and
`init-package` all fail until one exists, because there is no build key to sign with.

1. **Are you in one?** Walk up from cwd for a `.startos/` directory holding
   `build.key.pem` or a `schema:`-tagged `config.yaml`. That pair is the whole marker —
   a bare `.startos/` is not one, and the legacy global `~/.startos` never was.

   ```bash
   d=$PWD
   while [ "$d" != / ]; do
     if [ -f "$d/.startos/build.key.pem" ] || grep -qs '^schema:' "$d/.startos/config.yaml"; then
       echo "workspace root: $d"; break
     fi
     d=$(dirname "$d")
   done
   ```

2. **If there is no workspace, ask before creating one**, and follow
   `references/workspace-setup.md`. Scaffolding writes files into a directory the user
   chose for something else, clones ~75 MB, and generates a signing key that is the
   workspace's permanent identity — never do it unasked. Missing prerequisites are the
   same: report what's missing with the command that fixes it, and let the user run it.

3. **Confirm you will be able to install and verify** before you build anything. Phase 4
   needs a StartOS box the workspace can reach — `host.default` in the nearest
   `.startos/config.yaml` (which ships commented out), or the host `AGENTS.local.md`
   tells you to target — plus one `start-cli auth login` against it, and a backup target
   on it for the round-trip. A box is not required to package, but it is the only
   verification there is: find out now what you will be able to prove, and put the gaps
   on the gate rather than discovering them after the build.

4. **Is it already packaged?** A duplicate is the cheapest mistake there is to avoid.

   ```bash
   ls <workspace root> | grep -i <term>      # the workspace's own package repos
   start-cli -r prod registry package index --format json | jq -r '.packages | keys[]'
   start-cli -r https://community-registry.start9.com registry package index --format json | jq -r '.packages | keys[]'
   ```

   A near miss — a different frontend for the same backend, a fork of the same project,
   a package that would become a dependency — is not a stop. It is a fact for the gate.

5. **Decide nothing about where it lands yet.** A new package is a `<id>-startos/` repo
   under the workspace root, in the GitHub org that will publish it; `AGENTS.local.md`
   may prescribe a subdirectory or an org. Confirm at the gate, and don't scaffold before
   then.

## Phase 1 — Research the upstream

`references/upstream-research.md` is the checklist. Work it in full.

**Verify every fact with a tool.** `recipe-prebuilt-image.md`'s first rule — never
guess an image name, tag, or architecture — is this whole phase in miniature. An
unverified fact here becomes a package that fails on the box an hour later, long after
the gate closed.

**Probing the image is encouraged; leaving it running is not.** `docker manifest
inspect` and a `docker run --rm` under a `timeout` answer most questions about paths,
ports, entrypoints and init systems. Every container you start for a question: `--rm`,
bounded by a `timeout` or a one-shot command, published on `127.0.0.1` only, named with a
`scratch-` prefix, and gone by the end of the turn that started it. Never let a daemon
do real work you didn't ask for — a config check does not need a synced chain or a
populated index; run it in a mode that cannot start one.

**Stop and report instead of reaching the gate** when the research says the package
cannot be good: no `aarch64` image and no practical way to build one, a license or
trademark that forbids redistribution, a paid key required to run at all, or a hard
dependency on a hosted service. Say what blocks it and what the options are — that is a
complete answer, not a failure.

## Phase 2 — The gate

Deliver inline in the conversation. **Never write a file** — a research report is
exactly the kind of thing that wants to become one, and nobody asked for one.

1. **What it is** — a short paragraph, and why someone self-hosts it.
2. **What you found** — the research, organized, with sources. Facts, not prose.
3. **The shape you recommend** — image, interfaces, dependencies, actions, tasks,
   backup strategy, what ships and what doesn't — each with a one-line reason.
4. **The questions.**

`references/decision-gate.md` is the question catalog. How to ask:

- **Every question carries your recommendation**, so that "go with your recommendations"
  is a complete answer.
- **Discrete choices are numbered options, recommended one first and labelled as such.**
  Where your harness has a structured question tool (Claude Code's `AskUserQuestion`,
  at most four questions per call), use it for those; open-ended questions go in prose
  beneath.
- **Ask only what changes the package.** A question whose answers produce identical
  code is noise: decide it, and say what you decided.
- **Ask everything you will need.** Coming back mid-build for one more answer is the
  failure this phase exists to prevent.

## Phase 3 — Package it

**Scaffold; never hand-assemble by copying another package.**

```bash
cd <directory the package lives in>        # the workspace root unless AGENTS.local.md says otherwise
start-cli s9pk init-package "<Display Name>"
```

It normalizes the display name to an id, creates `<id>-startos/` in the current
directory, `git init`s it, and runs `npm install`. Check the id it will derive is the
one you want before running it — the directory name and the package id both follow from
that string.

Then work `new-package-checklist.md` top to bottom, pulling in each recipe the agreed
shape needs. What the checklist assumes, and a Start9 reviewer holds a package to:

- **The scaffold's `AGENTS.md`, `CLAUDE.md`, `tsconfig.json`, `.gitignore` and
  `.dockerignore` arrive correct — don't drift them.** Service-specific: `README.md`,
  `instructions.md`, `UPDATING.md`, `LICENSE`, `icon.svg`, and the `Makefile`'s
  `ARCHES`.
- **i18n is a deliverable, not a scaffold.** Every user-facing string through `i18n()` —
  `main.ts`, `interfaces.ts`, **and each action's name, description, and every result's
  title/message/value**. `i18n/dictionaries/translations.ts` populated for `es_ES`,
  `de_DE`, `pl_PL`, `fr_FR` across every index in `default.ts`; the manifest's short and
  long descriptions and the release notes in those same locales. Keep the
  `satisfies Record<string, LangDict>` — it is what forces per-locale completeness.
- **README and instructions get audited, not written once.** Open `writing-readmes.md`
  and `writing-instructions.md` and work their pre-publish checklists item by item. The
  README's heading set is an addressing scheme: verbatim names, prescribed order, no
  heading omitted — a section with nothing to say still says "None."
- **`instructions.md`'s `## Documentation` bullets are machine-parsed** — Start9's
  support indexer crawls each URL into the package's upstream-docs index, and a URL that
  classifies badly costs a wasted crawl on every run. `writing-instructions.md` §
  Choosing documentation URLs has the rules; check each URL against them before you
  write it.
- **The package owns the credential.** For a service with accounts, follow
  `recipe-admin-credentials.md` exactly: one action generates, stores, and returns it,
  an init watcher raises a `critical` task while it is unset, and the same action
  rotates it. Three shapes a reviewer rejects: a separate "show credentials" action
  (two owners, no reset path); the service's own first-run wizard left reachable, so
  whoever opens the address first claims the instance; a rotation that rewrites the
  store but never reaches the app. Drive the flow end to end — old credential rejected,
  new one accepted — before calling it done.
- **Run down every red flag.** A leftover file, a cast that fights the SDK (`as any`,
  `as unknown as`), a workaround comment, a deviation from the canonical packages — each
  is a bug or a misunderstanding until you can explain precisely why it is correct.
- **The icon is fetched, never drawn.** Upstream's own asset, ≤ 40 KiB. If there isn't a
  usable one, ship nothing invented and name the gap at hand-back.
- **`packageRepo` names the repo that will exist** — `<org>/<id>-startos`, the org the
  gate settled. `upstreamRepo` names the software project. Nothing has created the
  former; flag it at hand-back.
- **Point the workflows at the branch the repo will use** — `build.yml`'s PR target and
  `tagAndRelease.yml`'s push trigger. The `@master` in
  `uses: Start9Labs/start-technologies/...` is the monorepo's branch; leave it.
- **Leave `FREE_DISK_SPACE` off.** It is reactive, turned on only after a package has
  actually hit a disk-space failure in CI.
- **One version file.** The version lives in `startos/versions/current.ts`; a new
  package has no migrations and no other version files.

Iterate with a dirty tree. Don't commit between attempts.

## Phase 4 — Verify on a StartOS box

`references/verify-on-startos.md` is the protocol. Run it in full against the box Phase 0
confirmed, with whatever install command and host `AGENTS.local.md` prescribes.

Compiling is not working. A green `tsc` and a clean pack are the floor. The bar is:
installs clean, daemon starts, health checks go green, **every** interface is reachable,
**every** action runs end to end, tasks appear and clear, real data survives a restart,
backup and restore round-trip, and uninstall leaves nothing behind.

**No box is not a pass.** If Phase 0 found nothing to install to, build the package
(`make x86` or `make arm`), say plainly at hand-back that it has never run, and hand the
protocol over as the user's to drive. Report anything you could not make work as exactly
that. Never smooth it over, and never report green on the strength of `tsc`.

## Phase 5 — Hand back

Stop. **No commit, no push, no `gh repo create`, no pull request** — unless
`AGENTS.local.md` says how a finished package is handed back in this workspace, in
which case do that. Otherwise the package sits untracked in its freshly `git init`ed
repo, which is the user's review surface.

**Leave it installed** on the box so the user can drive it, and say so with the
uninstall command. Report inline:

- what the package is, and the shape you built to
- image and tag pinned, interfaces, actions, tasks, dependencies, what is backed up
- what you verified on the box, and how — or that nothing ran, and why
- what you could not make work
- what is left for the user: creating the GitHub repo, confirming `packageRepo`, the
  first commit
- anything you left behind that is not the package — a scratch container, a backup on
  the box's backup target, a domain registered on the box

### When the package is approved

**In a repo whose org sets `RELEASE_REGISTRY` — every `Start9Labs` and
`Start9-Community` repo — every non-Markdown push to `master` publishes the package.**
`tagAndRelease.yml` fires on the push and ships whatever is there under the package's
real id.

So `master` never receives the scaffold. What reaches it first is either the finished,
reviewed package as one commit, or a README alone with the package following as a pull
request — `AGENTS.local.md` says which shape this workspace uses. Never bootstrap the
repo with a scaffold `Initial commit` and the package as a PR on top: that shape looks
tidy, but it publishes a hello-world under the real id, with
`packageRepo: https://github.com/REPLACE_ME/…` and TODO descriptions, which Start9's
support indexer then reports as a broken package. It does not age out either — pruning
only compares revisions within one upstream version — so recovering means deindexing it
by hand.
