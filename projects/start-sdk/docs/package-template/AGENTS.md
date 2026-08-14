# AGENTS.md

This is a StartOS service-package repository — it builds a `.s9pk` for StartOS.

Develop it inside a StartOS packaging workspace created by `start-cli s9pk init-workspace`,
which provides the packaging guide and agent context one level up. If you're reading this in a
bare clone with no workspace, the full guide is at <https://docs.start9.com/packaging>.

**Start every task at the recipe index** — `../start-technologies/projects/start-sdk/docs/src/recipes.md`
(or <https://docs.start9.com/packaging/recipes.html>). It maps an intent ("prompt the user to create
admin credentials", "expose a web UI") to the constructs, the reference pages, and a named production
package to copy. Find the recipe before you read this package's neighbours: a package you reach by
grepping may be non-conformant, and the recipe outranks it.

Work this package's `TODO.md` from top to bottom. Keep `README.md` (the package's technical reference — the only one an AI support or administering agent reads) and `instructions.md` (end-user docs) in sync with your changes.

## This repo

<!--
TODO: only what someone *changing* this package needs and cannot get from
README.md or instructions.md. Those two are the richer sources on how the package
works and who it serves — restating them here creates a third copy that drifts.

What has no home in them, and belongs here:

  - repo mechanics — parallel version branches, a worktree layout, a vendored tree
  - prohibitions — a change that looks right and is not, plus the one clause saying why
  - extension points — where the next backend, interface, or migration gets added
  - naming traps — e.g. a package id that differs from the repo directory name
  - build or test invocations specific to this package

Most packages need one to four bullets. A simple one needs none — delete the
section rather than padding it. Remove this comment when you write yours.
-->
