# Publishing

Every `.s9pk` needs a registry to live in before it can be installed on a StartOS device. StartOS is deliberately flexible about which registry that is — you can run your own forever, submit to the Start9 Community Registry, or do both in parallel. Nothing about the packaging workflow requires you to distribute through Start9.

## Self-Hosted Registry

The fastest and most autonomous path is to run your own registry — install the `startos-registry` service on a StartOS device, point `start-cli` at it, and publish. See [Hosting a Registry](host-registry.md) for the full walkthrough (install, first-run setup, administration).

You can run a self-hosted registry in parallel with a Start9 Community submission: developers often keep an alpha/testing registry of their own while a more stable build is promoted through the community pipeline.

## Start9 Community Registry

If you want your package on Start9's official community registry, the current flow is email-driven. A developer portal with self-service submission and promotion is on the roadmap; until it ships, this is the interface.

The community registries, in promotion order:

- **community-beta** — <https://community-beta-registry.start9.com> — receives every PR-merge build automatically
- **community** (production) — <https://community-registry.start9.com> — promoted from beta on your go-ahead

### Initial Submission

1. **Email <submissions@start9.com>** with a link to your public GitHub repository.
2. Start9 **forks** your repo into the [Start9-Community GitHub organization](https://github.com/Start9-Community) and reviews it against this guide — correctness, conformance, docs, localization, CI. The review lands as a pull request on the fork, so you see every change before it merges.
3. **The fork is the upstream from that point on.** Anything left for you to fix, and every later version, is a PR against the fork — not your original repo.

### The Pipeline

Once your fork exists inside Start9-Community:

1. **Open a PR** against the fork with your changes.
2. **Merge** — when Start9 merges the PR, a workflow automatically builds, tags, and deploys the package to **community-beta**. You don't run any publish commands yourself; the automation handles it.
3. **Test the beta build.** This is where a release soaks: install it from community-beta, exercise it, and give other users a chance to.
4. **Promote to production** — when you're ready to ship broadly, email <submissions@start9.com> or open an issue on the fork. Start9 promotes the current beta build to **community**. The go-ahead is yours to give, not Start9's.

Every subsequent change or version bump is another PR through the same cycle — merge publishes to beta, email/issue promotes onward.

## Pre-Publish Checklist

Before publishing to your own registry — or before opening / updating a PR on the Start9-Community fork — walk through this. For community submissions, these checks must pass **before** you open the PR: the merge triggers the build, and anything wrong will ship directly to community-beta.

1. **Tag convention followed.** Your version tag matches [Git Tag Conventions](./versions.md#git-tag-conventions).
2. **All checks pass.** `tsc --noEmit`, tests, and the pack step must be green.
3. **README is current.** Every action, volume, port, dependency, and limitation matches the code. No version numbers anywhere — see [Writing READMEs](./writing-readmes.md).
4. **Tested end-to-end on StartOS.** Installed cleanly, service started, UI loaded (if applicable), health checks went green. Uninstall and reinstall to confirm teardown works.
