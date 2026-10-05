# Development Workflow

This page covers how to _behave_ while working on a package — the disciplines that apply to every change, no matter which SDK constructs you touch. The rest of the guide describes _what_ to build; this page describes _how_ to work while building it. These rules are the canonical home for the working discipline an AI coding agent should follow on every change.

## Keep README and instructions in sync

`README.md` and `instructions.md` are part of the package, not afterthoughts, and they track different things. `README.md` is the package's technical reference, and the only one an AI support or administering agent reads — update it for any change to how the package is built, structured, or behaves (a new or renamed action, an added or removed volume/port/interface/dependency, a changed default, a new feature or limitation). `instructions.md` is the end-user guide — update it whenever a change affects what the user sees or does. When a change touches both, update both in the same change.

Apply this loop on every change:

1. Make the code change.
2. Open `README.md` and `instructions.md`. Read what each says about the area you touched.
3. If either no longer matches the code, update it in the same change.
4. If a file is silent on the area and doesn't need to speak to it, leave it.

Don't skip step 2 on the theory that a change was "internal." If you're unsure whether a change is worth documenting, the doc check _is_ the answer: if neither file mentions the area, it was internal; if one does, your change probably affects that file.

See [Writing READMEs](./writing-readmes.md) and [Writing Instructions](./writing-instructions.md) for the content rules.

## Iterate with a dirty working tree

`start-cli s9pk pack` appends a `-modified` suffix to the version hash when the working tree is dirty. This is **purely informational** — the `.s9pk` works exactly the same. Do not commit between test attempts just to get a clean hash.

- Leave the tree dirty while iterating.
- When the package works end-to-end, make **one** clean commit — not a trail of `fix: X`, `fix: Y`, `fix: Z` fixup commits.
- If you've already accumulated fixups during a debug session, `git reset --soft HEAD~N` collapses them so you can recommit as one.

## Pre-existing errors are still errors

If `tsc`, a test, or the pack step fails — even on something unrelated to your change — the package does not pass. "Pre-existing" is not a pass condition; it is a signal that nobody has fixed the problem yet. Either fix it, or stop and flag it explicitly. Never report a run as green when any check was red.

### Reinstall before believing a type error

A `node_modules` that has drifted from `package-lock.json` produces errors that read exactly like real bugs in your code — a nullability complaint, a mismatch deep inside a dependency's own typings. Run `npm ci` and check whether the error survives before you spend time on it, and certainly before you change code to satisfy it.

The same drift ruins `git bisect`. If `node_modules` is shared across the checkouts you're bisecting — symlinked in, or left in place while the tree moves under it — every commit is judged against the same broken typings, and the bisect lands on an innocent commit with total confidence.

## Work one package at a time

Finish a change in one package before starting the next. When several packages need the same edit, that is a deliberate decision to make once and then apply, not a default to slip into: a mistake made in one package is a bug, and the same mistake cascaded across a fleet is an afternoon of reverts. Land the first one, confirm it builds and behaves, and only then propagate.

## Verify against reality, not against `tsc`

A clean `tsc` and a successful `start-cli s9pk pack` prove the code type-checks and the package builds. They prove **nothing** about whether the service runs, the web UI loads, logins work, or data persists. Type-checking a credential flow that has never accepted a login, or a daemon that mounts the wrong path, passes just as green as one that works.

Before reporting a feature as done, exercise it against a running service:

- **Install on a StartOS box** (or run the image directly) and confirm the daemon stays up — not just that it starts.
- **Use the actual feature.** If you wired up admin credentials, log in with them. If you mounted a data volume, write data and restart to confirm it survives. If you exposed a port, connect to it.
- **A feature you have only compiled is unverified.** Say so plainly — "builds clean; not yet installed/tested" — rather than implying it works.

### Inspecting a running install

To read a generated config or grep the application's own logs from inside a container:

```
start-cli package attach <id> -n <subcontainer-name> -- <cmd>
```

Select the subcontainer by **name** with `-n` — the name passed to `SubContainer.of` in `main.ts` — or by image with `-i`. `-s`/`--subcontainer` takes the internal **Guid**, not the name, so passing a name to it fails with "no matching subcontainers"; that is the most common way this command is got wrong. A service with more than one subcontainer requires a selector, and with none given `attach` falls back to an interactive picker that panics in a non-TTY shell — which is the missing selector surfacing, not a TTY requirement.

## Don't fabricate — verify or flag

When you don't know a fact, find it; don't invent it and move on. The failure mode to avoid is stating a guess with the confidence of a checked fact. Three places this bites hardest:

- **Image names and tags.** Confirm the repository and tag exist in the registry before pinning `dockerTag` — don't guess `org/name` from memory. (See [Package a Prebuilt Docker Image](recipe-prebuilt-image.md).)
- **Upstream internals** — config-file formats, credential hashing schemes, file paths. Read them from the app or its docs, or apply them through the app's own CLI/API. Hand-writing a format you assumed (e.g. a bare hash where the app expects salted PBKDF2) fails silently.
- **Brand assets.** Never ship an invented `icon.svg` or logo. Fetch the real asset from upstream, or leave the placeholder and say that it still needs the real icon.

When you can't verify something, say so — raise it as an open question, or open an issue if it needs tracking beyond this session. Don't paper over it with confident prose in the README.

## Fix what you find; file only what needs deciding

A defect you spot while you already have the package open is a fix in the branch you are already on, not a report for someone else to pick up. That holds whether or not it is related to what you came to do: you have the code in front of you and the context to be sure, and the next person has neither. Describe what you fixed, and why, in the PR body.

Open **a GitHub issue on the package repo** when the call is not yours to make — you cannot pin the cause down, two defensible fixes exist and choosing between them needs a human, or the change is too large to ride on the work in hand. Say what you found and what you would need decided. Then reference the issue in a line of the PR body if a PR is open; don't write the finding out in full in both places, or the two copies immediately start to disagree.

The tracker in the other direction is not an invitation. An open issue is somebody's report, not a queue you may pick from — take one on when you were asked to, or when it carries the `Approved` label, which is a maintainer saying it is ready for a PR. Then implement it in the branch you already have and put `Closes #<n>` in the PR body.

Nothing else in the repo is a place to record work. Don't create a `TODO.md`, a `NOTES.md`, a `PLAN.md`, or a results log at the bottom of a doc: a file like that is invisible to everyone who isn't reading the repo, it accumulates session notes nobody can act on, and it goes stale the moment the session ends.

What is neither a fix nor an issue: what you verified, what you tried, what you decided and why. That is the commit message and the PR body's job, where it stays attached to the change that motivated it.

## Search the SDK before deciding something is impossible

Before concluding the SDK can't do what you need — or working around a limitation you've assumed — grep the installed type definitions: `node_modules/@start9labs/start-sdk/**/*.d.ts`. The SDK exposes far more than the recipes show, and the option you want is often a field on a type you're already using (this is how `runAsInit` is found, for example). "The SDK doesn't support X" is a claim to verify in the types, not a conclusion to reach from the docs alone. If it genuinely isn't there, say so and explain the workaround — don't silently route around a capability that exists.

## A comment is not evidence

A comment asserting what an SDK call does — in a package you're reading, in a code review, in this guide's own prose — is a claim, not a fact. Confirm it against the reference page, the installed types, or the SDK source before you accept it, repeat it, or write code that depends on it. Wrong claims about semantics propagate: one plausible sentence gets copied into the next package, then quoted in a review, then built into a plan.

`merge(effects, {})` is the standing example. It has variously been described as rewriting the file, as cleaning or stripping it, and as a no-op against an existing one. Every reading was plausible; none was correct — see [What an Empty merge() Does](./file-models.md#what-an-empty-merge-does).

### Fetch a package before reading it as a reference

The same applies to a package you open to derive behavior from — a dependency's volume path, a credential scheme, a shared pattern, a version pin. Fetch it first. A checkout you cloned weeks ago shows code the package has since changed, and it will mislead you with total confidence: reading a dependency's API key after upstream dropped it produces an integration that type-checks, builds, and cannot work.

## Read the monorepo source only when the guide can't answer

Your workspace's `start-technologies/` is a checkout of the whole Start9 monorepo, so the **SDK source** (`projects/start-sdk/lib`) and the **StartOS source** (`projects/start-os`, and the shared core in `shared-libs/`) are already on disk — behind the recipes, the reference pages, real packages, and the installed `@start9labs/start-sdk` types.

This is a **last resort, not a starting point.** Drop into the source only to answer a specific question those layers can't — exactly what an SDK call does, how an OS effect behaves — and read the one file that settles it instead of browsing.

When the answer turns out to be a bug rather than a misunderstanding, fix it there: that checkout is a full git repo, so you can branch, commit, and open a pull request without leaving the workspace. Branch from `origin/master` — the checkout itself sits on `live-docs`, which carries what is published — and switch it back when you're done.

## Don't create unnecessary version files

Most version bumps edit `startos/versions/current.ts` in place — change the `version` and `releaseNotes`, leave `index.ts` and the filename alone. A new file is spun off only when the version already in `current.ts` carries a migration, which stays with the version that introduced it rather than riding forward into its successor. See [Versions — When to Create a New Version File](./versions.md#when-to-create-a-new-version-file) for the rule, and [Release Notes](./versions.md#release-notes) for how to write the notes that accompany a bump.
