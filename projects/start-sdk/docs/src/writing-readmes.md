# Writing Service READMEs

Every StartOS package README documents **how your service on StartOS differs from the upstream version**. Everything else, a reader can find in the upstream docs. It is also the file that AI agents read to support, operate, and contribute to your package, so its structure is load-bearing in a way an ordinary repository README's is not.

## Who reads this file

Four readers consume a package's documentation, and they are **nested** — each reads a prefix of the same list, not a file of its own.

| Reader                                   | Reads                                                        |
| ---------------------------------------- | ------------------------------------------------------------ |
| The user running the service             | `instructions.md`                                            |
| An AI support agent                      | `instructions.md` + `README.md`                              |
| An AI assistant administering the server | `instructions.md` + `README.md` + the live action/health ABI |
| A developer or AI changing the package   | `instructions.md` + `README.md` + `AGENTS.md`                |

Two consequences:

- **The README is the only technical file a support agent gets.** It reads this and `instructions.md` from your repository and nothing else — not `AGENTS.md`, not your source. If a fact is needed to diagnose a user's problem, it belongs here.
- **Where readers conflict, the operable surface wins.** An agent wants predictable structure it can address; a developer wants prose. Both are served by fixed headings with real writing underneath — not by prose that wanders across section boundaries.

`instructions.md` is for the user (see [Writing Service Instructions](writing-instructions.md)); `AGENTS.md` is for whoever changes the package, and repeats nothing this file says (see [Project Structure — AGENTS.md](project-structure.md#agentsmd-and-claudemd)).

## Guiding principles

**Do not duplicate upstream documentation.** If something is not mentioned in your README, readers should assume the upstream docs are accurate.

**State differences, never sameness.** The scoping note at the top of the file already says that anything this document does not mention behaves as upstream — a complete statement that stays accurate for free. An enumeration of unchanged features cannot match it: the list is unbounded, it goes stale as upstream grows, and it inverts the note's logic, because once such a list exists a feature missing from it reads as changed. Where a reader would reasonably expect the package to have broken something and it did not, say so beside the thing that would have broken it.

**Do not restate `instructions.md`.** Every reader of this file also has that one, so a fact stated in both is a copy that will eventually disagree with itself. Upstream documentation links in particular belong _only_ in `instructions.md`'s `## Documentation` section — that section is parsed to source a package's upstream docs, so a second copy here is the one nothing validates and the one that rots when upstream moves. Say what the package does differently; let `instructions.md` say how to use it.

**Do not re-encode what StartOS can introspect.** An agent administering your service already has every action's id, name, description, warning, visibility, `allowedStatuses` and input schema from the OS, along with health-check ids and live status. Restating those here adds a second copy that goes stale and costs the agent context to read. Document instead what the ABI cannot express: **when** to run a thing, what it **costs**, whether it is **safe to repeat**, what **state** it changes, and which **symptom** it resolves.

**The heading set is fixed, not suggested.** Section headings are how an agent retrieves part of a README without loading all of it, so they are an addressing scheme. A package that renames `## Actions` to `## Available Actions` does not fail loudly — it silently degrades retrieval to "load the whole file". Use the headings below verbatim, in this order. No heading is optional: a section with nothing to say still says "None." An empty section is a fact worth stating — "no config file on disk", "no dependencies", "no actions", "no tasks" each answer a question outright, where a missing heading is ambiguous, since the reader cannot tell an absent section from an unwritten one and an agent addressing that heading gets nothing back either way.

The order runs in four groups: **what the package is made of** (runtime, volumes, file models, dependencies, interfaces), **how it behaves** (install, actions, tasks, health, backups), **what to expect when it doesn't** (limitations), then the machine-readable summary. Keep a new section inside the group it belongs to.

Nothing here is about contributing to the package. Build workflow, repo conventions, and the packaging guide live in `AGENTS.md` — restating them here produces a section identical in every package, useful to none of this file's readers.

**Open every H2 with prose.** One or two sentences between the heading and the first table or subsection, describing what the section covers. This text is extracted as the section's summary in the generated index, and it is what an agent reads to decide whether to fetch the section at all.

**No version numbers anywhere.** Not upstream versions, not image tags, not dependency version ranges. The manifest and `setupDependencies()` are the source of truth; a copy here is wrong from the next bump onward.

## Required structure

````markdown
<p align="center">
  <img src="icon.svg" alt="[Service Name] Logo" width="21%">
</p>

# [Service Name] on StartOS

> Everything not listed in this document should behave the same as upstream
> [Service Name]. If a feature, setting, or behavior is not mentioned here,
> the upstream documentation is accurate and fully applicable — see the
> Documentation section of `instructions.md` for links.

[Brief description of what the service does and link to upstream repo]

---

## Table of Contents

[Links to each section — must include all sections present in the README]

---

## Image and Container Runtime

## Volume and Data Layout

## File Models

## Dependencies

## Network Access and Interfaces

## Installation and First-Run Flow

## Actions

## Tasks

## Health Checks

## Backups and Restore

## Limitations and Differences

---

## Quick Reference for AI Consumers

```yaml
package_id: string
image: registry/name # never a tag
architectures: [list]
subcontainers: [list]
volumes:
  volume_name: mount_path
file_models:
  - path/to/config.json
startos_managed_env_vars:
  - VAR_NAME
dependencies: [list or "none"]
interfaces:
  interface_id: { type: ui | api | p2p, port: number }
actions:
  - action-id
tasks:
  - { action: action-id, severity: critical | important | optional }
health_checks:
  - check-id
```
````

## Sections

### Logo

Every README begins with the service icon centered above the title:

```html
<p align="center">
  <img src="icon.svg" alt="[Service Name] Logo" width="21%" />
</p>
```

Adjust `src` to the actual icon filename.

### Image and Container Runtime

Where the image comes from and how it runs.

| What to Document | Example                                   |
| ---------------- | ----------------------------------------- |
| Image source     | Upstream unmodified, or custom Dockerfile |
| Architectures    | x86_64, aarch64, riscv64                  |
| Entrypoint       | Default or custom                         |

Name the subcontainers the package runs and what each is for. An agent needs them to attach to a running install (`start-cli package attach <id> -n <subcontainer-name>`), and it cannot introspect them.

### Volume and Data Layout

Where the service's data lives.

| What to Document | Example                              |
| ---------------- | ------------------------------------ |
| Volume names     | `main`, `data`, `config`             |
| Mount points     | `/data`, `/config`                   |
| StartOS files    | `store.json` for persistent settings |
| Database         | Embedded SQLite vs external          |

### File Models

Which configuration files the package owns, how they get their values, and what happens to an edit the user makes by hand.

For each model: the file it maps to and its format, how it is seeded (merged defaults at install, generated from a resolved dependency address, written by an action), what rewrites it afterwards and when, and whether a hand edit survives.

**Ownership is the part that generates support tickets.** "Why did my setting revert" is almost always a value the package re-asserts on every start. Say plainly which keys are re-asserted, which are seeded once and then belong to the user, and whether an action exists to hand a key back.

Note `store.json` if the package keeps one — it holds StartOS-side state rather than upstream configuration, and it is usually what makes an ownership decision survive a restart.

Where a setting is delivered by environment variable instead of a file, say so and say why: a variable the application re-reads on every launch behaves nothing like one it consumes only on the launch that finds its value unset, and treating the second kind as authoritative is a common packaging bug. A configuration file the package writes without a model belongs in this section too.

If the package writes no configuration at all, state "None" and say so plainly — that there is nothing on disk to inspect or correct is an answer, and a useful one.

### Dependencies

What this service needs from other services.

For each dependency: its name, whether it is required or optional, the health checks that must pass before this service starts, any mounted volume (with mount point and read-only status), and why it is needed.

Do **not** restate the version range — `setupDependencies()` declares it, and a copy goes stale the first time you raise the floor. If the service has no dependencies, state "None" explicitly.

### Network Access and Interfaces

What the service exposes. For each interface: its id, type (`ui`/`api`/`p2p`), port, protocol, and purpose.

Describe what the interface serves, not how StartOS interface controls work — LAN/Tor/domain addressing is a platform feature documented once, not per package.

### Installation and First-Run Flow

How setup differs from upstream. Document if your package skips a setup wizard, auto-generates credentials, pre-configures settings, boots the service during init, or creates tasks for initial setup — and any ordering constraint the user or an agent must respect.

### Actions

What can be done to the service, and when.

The OS supplies each action's id, name, description, warning, visibility, `allowedStatuses` and input schema. Do not restate them. For each **user-facing** action, document what the metadata cannot carry:

- **When to run it** — the situation that calls for it.
- **What it changes** — which files, volumes, or application state.
- **Cost** — roughly how long it takes, and whether it interrupts service.
- **Repeat safety** — idempotent, a no-op while already running, or destructive on a second run.
- **What happens next** — restarts, where to watch progress.
- **Outputs** — credentials or values the caller receives.

Flag actions with `visibility: 'hidden'` as not user-facing, so a support agent never tells a user to run one. Where an action exists to satisfy a task, leave the trigger and clearing rules to [Tasks](#tasks) rather than describing them twice. If the package declares no actions, state "None".

### Tasks

What the service asks the user to do, and what makes the prompt go away.

StartOS reports which tasks are _currently_ raised, but the condition that raises one and the thing that clears it are your package's logic and cannot be introspected. This is the section that answers "my service won't start and I can't press anything" — a `critical` task blocks startup and suspends the ordinary controls, which is among the most common support reports a package generates.

For each task the package creates:

- **What raises it** — the condition, not the `reason` string the UI already shows.
- **Severity** — `critical` (blocks the service from starting), `important` (prominent, non-blocking), or `optional`.
- **What clears it** — running the target action, a configuration reaching an acceptable state, or the underlying condition resolving on its own. Say whether it can return.
- **Where it appears** — for a dependency task (`createTask`), name the dependency and its action. The user sees that prompt on _another_ service's page, and nothing there explains which package asked for it.

If the package creates no tasks, state "None" — that the service is never held on a prompt, and its ordinary controls are always available, is itself worth saying.

### Health Checks

How to tell whether the service is working.

For each check: what it probes, its grace period, and — most importantly — **what a failure means and what to do about it.** "Not ready" is a state an agent can already read; the diagnostic value is in what distinguishes a slow start from a real fault.

### Backups and Restore

What survives a backup, and what a restored instance has to rebuild.

Lead with the **strategy**, because it decides what the guarantee actually is: volumes copied wholesale (`ofVolumes`), a database dumped and replayed rather than copied (`withPgDump` / `withMysqlDump`), or a mix. A volume that is dumped is not a volume that is backed up — its files are never captured, and restore reconstructs it by starting the engine and replaying the dump. Saying only that it is "included" tells a reader the opposite of what happens.

Then: what is deliberately excluded and why (a cache or an index that rebuilds is a feature, not a gap), and what a restored instance still has to do before it is usable — a resync from a dependency, a credential to re-enter, a dependency that must be present first.

### Limitations and Differences

A numbered list of what does not work, works differently, or is unavailable compared to upstream — including unsupported dependencies and deliberately disabled features.

### Quick Reference for AI Consumers

A YAML block summarizing the package's operable surface — the fields in the template above. It exists so an agent can establish what the package _is_ in one cheap read before deciding which prose section to fetch.

Its keys mirror the sections, in section order, so it doubles as an index. A section earns a key only when its content is a flat enumeration; where the meaningful fact is a behavior rather than a list, the section stays narrative and gets no key — installation, backups and limitations are all in that group. A key that flattens a behavior into a list is worse than no key, because it reads as precise: `backups: {included, excluded}` would file a dumped database under "included" and tell a reader their volume is captured when it never is.

Include no versions of any kind: not `upstream_version`, not image tags, not dependency version constraints.

## Pre-publish checklist

- [ ] Centered logo header at the top of the file
- [ ] Upstream-behavior scoping note at the top; no upstream documentation links (those live in `instructions.md`)
- [ ] Nothing restated from `instructions.md`
- [ ] Headings match the required set, verbatim and in order
- [ ] Every H2 opens with one or two sentences of prose before any table or subsection
- [ ] All volumes and mount points documented
- [ ] Subcontainer names documented
- [ ] Every user-facing action covers when to run it, what it changes, its cost, and its repeat safety — without restating the OS metadata
- [ ] Hidden actions flagged as not user-facing
- [ ] Every task documented with what raises it, its severity, and what clears it — or "None" stated explicitly
- [ ] Health-check failures explained, not just listed
- [ ] All dependencies documented (or "None" stated explicitly)
- [ ] Every file model documented — how it is seeded, what rewrites it, and whether a hand edit survives
- [ ] All limitations listed explicitly
- [ ] YAML quick reference block present and version-free
- [ ] No version numbers anywhere — upstream version, image tags, dependency ranges
- [ ] Documented features match actual behavior, verified against a running install
