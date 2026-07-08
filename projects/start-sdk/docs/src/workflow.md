# Development Workflow

This page covers how to _behave_ while working on a package — the disciplines that apply to every change, no matter which SDK constructs you touch. The rest of the guide describes _what_ to build; this page describes _how_ to work while building it. These rules are the canonical home for the working discipline an AI coding agent should follow on every task.

## Keep README and instructions in sync

`README.md` and `instructions.md` are part of the package, not afterthoughts, and they track different things. `README.md` is the architectural reference for developers and AI — update it for any change to how the package is built, structured, or behaves (a new or renamed action, an added or removed volume/port/interface/dependency, a changed default, a new feature or limitation). `instructions.md` is the end-user guide — update it whenever a change affects what the user sees or does. When a change touches both, update both in the same change.

Apply this loop on every task:

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

## Verify against reality, not against `tsc`

A clean `tsc` and a successful `start-cli s9pk pack` prove the code type-checks and the package builds. They prove **nothing** about whether the service runs, the web UI loads, logins work, or data persists. Type-checking a credential flow that has never accepted a login, or a daemon that mounts the wrong path, passes just as green as one that works.

Before reporting a feature as done, exercise it against a running service:

- **Install on a StartOS box** (or run the image directly) and confirm the daemon stays up — not just that it starts.
- **Use the actual feature.** If you wired up admin credentials, log in with them. If you mounted a data volume, write data and restart to confirm it survives. If you exposed a port, connect to it.
- **A feature you have only compiled is unverified.** Say so plainly — "builds clean; not yet installed/tested" — rather than implying it works.

## Don't fabricate — verify or flag

When you don't know a fact, find it; don't invent it and move on. The failure mode to avoid is stating a guess with the confidence of a checked fact. Three places this bites hardest:

- **Image names and tags.** Confirm the repository and tag exist in the registry before pinning `dockerTag` — don't guess `org/name` from memory. (See [Package a Prebuilt Docker Image](recipe-prebuilt-image.md).)
- **Upstream internals** — config-file formats, credential hashing schemes, file paths. Read them from the app or its docs, or apply them through the app's own CLI/API. Hand-writing a format you assumed (e.g. a bare hash where the app expects salted PBKDF2) fails silently.
- **Brand assets.** Never ship an invented `icon.svg` or logo. Fetch the real asset from upstream, or leave the placeholder and flag that it still needs the real icon.

When you can't verify something, surface it as an open question or a `TODO.md` item — don't paper over it with confident prose in the README.

## Search the SDK before deciding something is impossible

Before concluding the SDK can't do what you need — or working around a limitation you've assumed — grep the installed type definitions: `node_modules/@start9labs/start-sdk/**/*.d.ts`. The SDK exposes far more than the recipes show, and the option you want is often a field on a type you're already using (this is how `runAsInit` is found, for example). "The SDK doesn't support X" is a claim to verify in the types, not a conclusion to reach from the docs alone. If it genuinely isn't there, say so and explain the workaround — don't silently route around a capability that exists.

## Read the monorepo source only when the guide can't answer

Your workspace's `start-technologies/` is a sparse checkout of the Start9 monorepo — by default only the packaging guide (`projects/start-sdk/docs`) is materialized, but the full **SDK source** (`projects/start-sdk/lib`) and **StartOS source** (`projects/start-os`, and the shared core in `shared-libs/`) are one command away when you need them — past the recipes, reference pages, real packages, and the installed `@start9labs/start-sdk` types.

This is a **last resort, not a starting point.** Drop into the source only to answer a specific question those layers can't — exactly what an SDK call does, how an OS effect behaves — and read the one file that settles it instead of browsing. Fetch a path that isn't checked out with `git -C start-technologies sparse-checkout add <path>`.

## Don't create unnecessary version files

Most version bumps edit `startos/versions/current.ts` in place — change the `version` and `releaseNotes`, leave `index.ts` and the filename alone. A new file is only spun off when the bump carries a migration. See [Versions — When to Create a New Version File](./versions.md#when-to-create-a-new-version-file) for the rule, and [Release Notes](./versions.md#release-notes) for how to write the notes that accompany a bump.
