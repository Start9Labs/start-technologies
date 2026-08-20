# Maintaining a Package

The rest of this guide is about making _a_ change: which construct to reach for, how to write it, how to verify it. This page is about the package as something that keeps existing — the branches it accumulates, the upstream it chases, and the sibling packages it depends on. None of it matters much on the day you scaffold a package, and all of it matters by the fiftieth release.

## Branches

A package has one **base branch** — the integration branch its CI is wired to. `master` and `main` are both fine; what matters is that the branch the repo actually uses is the branch its workflows name. `build.yml`'s PR target and `tagAndRelease.yml`'s push trigger both have to point at it, and a package sitting on `main` whose workflows still say `master` silently never builds a PR and never releases. See [Project Structure — .github/workflows/](./project-structure.md#githubworkflows).

Ordinary work targets the base branch: cut a feature branch, open a pull request, merge, and let the branch be deleted.

### `next`, the long-lived iteration branch

Alongside the base sits **`next`** — a branch that is never deleted and keeps accumulating work after each pull request lands. It exists so a package can be iterated on continuously without every increment having to be release-ready, and so a chain of dependent changes can build on each other before any of them ship.

[`syncNext.yml`](./project-structure.md#githubworkflows) keeps it honest: every push to the base is carried onto `next`, so it never falls behind what has already shipped. You do not have to create `next` yourself — the first run makes one at the base tip.

**After `next` picks up the base, re-check the version it claims.** The base moves on its own, and it can land — and release — the very work `next` is carrying. Two things go wrong, and neither announces itself:

- **The version is already published.** If `next` claimed `1.36.0:2` while the base was on `:1`, and the base has since released `1.36.0:2`, publishing `next` ships _different bytes under a live version_. Query the registry for the package's existing versions and take the next free revision.
- **The release notes describe work that already shipped.** When the base landed the same submodule bump or upstream refresh independently, `next`'s own commit is a no-op after the sync, but its notes still announce the fix. Drop the redundant commit and rewrite the notes to describe only what this version adds on top.

### Merge a long-lived branch with a merge commit, never a squash

This is the one branch rule that is easy to get wrong, because the habit runs the other way.

When you merge `next` into its base, use **`gh pr merge --merge`** — "Create a merge commit" in the web UI. The merge commit's second parent is `next`'s tip, which leaves `next` a true ancestor of the base, and the next sync fast-forwards it.

**Squashing re-lands the same content under a brand-new commit.** The base ends up with the changes but not the commits, so `next` is left carrying history the base will never contain — permanently diverged, with every later sync having to merge around it. Rebase-merging diverges for the same reason.

Feature branches are the opposite case: they are deleted on merge, so squash them as usual. The rule is about the head branch's **lifetime**, not about the repository. Anything long-lived — `next`, a release line, the scratch branch `syncNext` opens when a sync conflicts — takes a merge commit.

## Parallel release lines

Some packages ship more than one line at once: two upstream major versions that both still receive updates, or two build flavors of the same software. Each line gets its own base branch, and each base branch gets its own paired iteration branch named **`next/<base>`**.

The slash order is forced by git, not chosen. Refs are paths, so while a branch named `29.x` exists, no branch `29.x/next` can exist beside it — a ref cannot be both a file and a directory. `next/29.x` is the form that works.

> [!IMPORTANT]
> Every operation applies to **every** line, not just the one you have checked out. A version bump, a dependency refresh, a README correction, a security fix — if it belongs on one line it almost certainly belongs on the others. `git branch -r` tells you what the package maintains; `git worktree list` is the ergonomic way to keep several checked out at once.

For example, a package wrapping a widely-deployed daemon might carry `28.x` through `31.x` as base branches, with `next/28.x` … `next/31.x` paired to them, plus a separate line for a variant build of the same software. Users on the older line keep receiving fixes without being forced onto a major upgrade, and the packaging work happens once per line rather than once per release.

If your package only ever tracks one upstream line — most do — ignore all of this. A plain `next` is the whole story.

## Chasing upstream

`UPDATING.md` at the package root is the package-specific recipe: where its version string lives, which registry and tag format to look for, and exactly which field to edit. Follow it. It is also **not automatically correct** — if its command contradicts what you can actually observe, trust the observation, fix the file in the same change, and say so in the pull request.

Four disciplines apply on top of it.

### A release existing does not mean the artifact exists

**Verify that the thing you are about to pin actually resolves, before you pin it.** Upstreams tag a release and fail to publish the image often enough that this is a routine failure, not an edge case — and the symptom lands on users of the package, not on you, because the manifest type-checks and only the build fails.

- For a `dockerTag`, confirm the exact tag resolves. `docker manifest inspect <image>:<tag>` works, but anonymous Docker Hub requests are rate-limited and a throttled response is indistinguishable from a missing tag — prefer the registry's tag API (`https://hub.docker.com/v2/repositories/<namespace>/<repo>/tags/<tag>` for Docker Hub).
- For a source build, confirm the git tag exists.
- If the package pins **several** images that move together, check every one.
- If the newest release has no usable artifact, target the newest one that does, and say so in the pull request.

### Skip prereleases, and don't trust "Latest"

GitHub's "Latest" badge is unreliable in both directions: it can sit on a prerelease, and it can lag behind the newest stable. Read the tag list rather than the release page, and pin a stable release unless the package deliberately tracks a prerelease line.

### Scale scrutiny to the size of the jump

| Jump                      | What it needs                                                                                            |
| ------------------------- | -------------------------------------------------------------------------------------------------------- |
| **Patch** (1.2.3 → 1.2.4) | Low risk. Bump, verify the build, move on.                                                               |
| **Minor** (1.2.x → 1.3.0) | Read the full changelog. Look for deprecations and behavior changes; note new features worth surfacing.  |
| **Major** (1.x → 2.0)     | Read the changelog, release notes, and any migration guide. Inspect code diffs where the notes are thin. |

A major bump is also where you ask whether the package needs a [data migration](./recipe-version-migrations.md) — and whether the version currently in `current.ts` carries one that has to be spun off first. See [Versions — When to Create a New Version File](./versions.md#when-to-create-a-new-version-file).

### When the packaging repo is itself a fork

To sync a fork, you need its **fork parent**, which the GitHub API knows:

```sh
gh api repos/<owner>/<repo> --jq '.parent.full_name'
```

Do **not** use the manifest's `upstreamRepo` for this. That field points at the upstream _software_ project, which is a different repository from the packaging repo you forked — using it for a fork sync merges an unrelated history.

## Depending on another package's repo

A package can depend on another package's repo through npm, to reuse its exported constants and types:

```json
"dependencies": {
  "some-service-startos": "github:<org>/some-service-startos#next"
}
```

### Track the iteration branch, and let the lockfile do the pinning

Pin the git dependency at **`#next`** (or `#next/<base>` for a package with parallel lines) rather than at the base branch, so your package compiles against the sibling's newest types instead of the last integrated ones.

What makes a moving branch safe is the **lockfile**, not the specifier. `package-lock.json` records an exact commit, and CI installs with `npm ci`, so every build is reproducible. The branch only decides what a future `npm update` resolves _to_ — and that arrives as a reviewable lockfile diff rather than as a silent change.

> [!WARNING]
> `npm update` is a **no-op on a git-ref dependency**. npm considers `github:org/repo#next` already satisfied and will not re-fetch it, so the pin stays stale however many times you run it. To actually refresh: delete the git-resolved entries and their subtrees from `package-lock.json`, then `npm install` to re-resolve from `package.json`. That refreshes direct and transitive git dependencies together, which naming one package on the command line does not.

### Read the lockfile diff as code review, not as a version bump

Whether a sibling's code ends up in your `.s9pk` depends on how you import it, because the bundler tree-shakes what it can:

- A **type-only** use (`typeof manifest`) sits in a type position and is erased entirely.
- A **scalar constant** (a port number, a path) is inlined as its value; nothing else comes with it.
- A **value object** pulls its whole reachable graph in. Importing one exported config object can drag an entire configuration spec into your package — and then a sibling's changes become your package's changes.

So a lockfile diff that moves a sibling commit can change what ships. Check what actually landed:

```sh
start-cli s9pk inspect <package>.s9pk cat javascript.squashfs | unsquashfs -d out /dev/stdin
```

### Keep one copy of the SDK

When a sibling pins an older `@start9labs/start-sdk` than yours, npm hoists two copies, and generic helpers start failing to type-check in ways that read as bugs in your own code — a payload "not assignable to type `never`" is the usual shape, because the imported value no longer structurally matches the generic it is being matched against.

Force a single copy with an override, and remove it once the sibling catches up:

```json
"overrides": { "@start9labs/start-sdk": "<version>" }
```
