# Complexity tracking

A way to see what a change does to this repo's complexity, and to watch the figure move over
time. It gates nothing. The numbers are context for whoever is making the change — an
unexpected rise is a symptom worth looking at, and one the author should be able to explain or
refute. Every number below was measured on `7e58f9d45`.

## Why now

Same code, path-normalized across three directory reorgs, so the monorepo consolidation is
excluded — this is the StartOS Rust backend plus the web UI and nothing else:

| date       | backend KB | web UI KB |  sum | delta |
| ---------- | ---------: | --------: | ---: | ----: |
| 2025-03-01 |       1686 |       869 | 2555 |     — |
| 2025-09-01 |       1779 |       956 | 2735 |   +0% |
| 2025-12-01 |       2107 |       989 | 3096 |  +13% |
| 2026-04-01 |       2334 |      1033 | 3367 |   +3% |
| 2026-06-01 |       2513 |      1076 | 3589 |   +6% |
| 2026-08-27 |       3382 |      1098 | 4480 |  +24% |

Fifteen months to 2026-06 added 40%. The eleven weeks after it added 25%. The growth is
organic — no new top-level module, file count 324 → 356 — and concentrated: `net` went
468 → 965 KB, `tunnel` 90 → 310 KB. Over the same window commit volume went from roughly
20/month to 130–300/month, and Helix became the third-largest author, 208 of about 1,100
commits in twelve months.

None of that argues against the work. It argues that nobody is asked what it costs.
Zero of 566 merged PR bodies contain the string `complexit`.

## What gets measured

**Cognitive complexity per function**, via `bca` (tree-sitter, real Rust and TypeScript
grammars). Not cyclomatic complexity, and the difference is the whole argument.

Probing both against a synthetic file settles it:

| construct              | cyclomatic | cognitive | which is right                        |
| ---------------------- | ---------: | --------: | ------------------------------------- |
| 20-arm `match`         |         21 |     **1** | cognitive — a flat table is read once |
| generic `where` bounds |          1 |     **0** | cognitive                             |
| five `?` operators     |          6 |     **0** | cognitive — `?` is not cognitive load |
| four-deep nested `if`  |          5 |    **10** | cognitive — nesting is superlinear    |

Cyclomatic complexity ranks `error.rs::as_str` — an 84-line enum-to-string `match` — as the
worst function in the repo. It is the least risky code we have. Cognitive complexity does not
rank it at all. Lizard, the obvious cheap alternative, is worse still on Rust: its reader
never counts match arms and does count every `?` and every `where`, so adopting it would push
authors away from idiomatic error propagation and toward giant match statements. It also drops
functions silently when a TypeScript object key is named `interface`.

For TypeScript alone, eslint is a real alternative and cheaper than it looks — eslint 9 and
typescript-eslint are already bundled dependencies of `@start9labs/start-sdk`, with a flat
config at `projects/start-sdk/eslint.config.base.mjs` and a lint gate in `s9pk.mk`, so the
dependency is shipped, just never aimed at this repo's own sources. It lints 1,146 TS files in
2.3 s with no tsconfig and no parse errors. One binary covering both languages still wins here
(the two agree closely, Spearman 0.94), but if the TS half is ever split out, eslint is the
tool and `sonarjs/cognitive-complexity` is the rule.

The census covers `shared-libs/` and `projects/` — the code that ships. Build tooling,
`scripts/`, CI and the repo docs are outside it, which is why this RFC's own branch reports a
delta of zero.

Three deliberate exclusions: inline `#[cfg(test)]` items are stripped before measuring (this
repo has 1,150 `#[test]` functions and only 6,249 lines in dedicated test files, so a
path-based rule would fail and a PR adding good tests would read as adding complexity);
generated trees are skipped (`osBindings`, `locales`, `exver.ts`, `dist`, `target`); and the
census refuses to report when over 0.5% of AST nodes are parse errors, because grammar rot is
otherwise silent — a file the parser cannot read yields no functions rather than an error.

The margin there is large. `bca` finds **1 parse error in 2,681,337 nodes** across this repo.
The engine it forked finds 240, clustered in six Rust files on generic associated types and
`impl Trait` in argument position — syntax that postdates its January 2023 grammars. Keeping
current with the language is most of what the fork buys.

**The parser does not expand macros, and that is a hole.** Wrapping a body in `macro_rules!`
takes it from cognitive 15 to **0** and cyclomatic 11 to 1 — measured, identical logic. The
repo already carries `macro_rules!`, so the evasion would read as native. There is no fix
inside this tool; the report therefore prints the total number of lines inside macro bodies
(368 today) so that moving code there is at least visible. Note clippy has the mirror defect
in the other direction — it measures macro-_expanded_ HIR, so with 2,871 `t!()` i18n call
sites its ranking tracks i18n density rather than code, correlating with a real cognitive
metric at Spearman 0.435 and sharing 9 of its top 50.

## The tool

**`big-code-analysis` (`bca`)**, MPL-2.0 — a maintained fork of Mozilla's `rust-code-analysis`,
which is the only lineage that computes cognitive complexity locally for both Rust and
TypeScript. One binary, one pass, both languages: 527 Rust files and 759 TypeScript in 0.35 s
with no build and no `npm install`. It is pinned by sha256 against the upstream release
checksums and fetched into `build/complexity/bin/`, never committed; `cargo install` covers
platforms with no published binary.

Three of its flags do work this repo would otherwise need bespoke code for. `--exclude-tests`
skips `#[test]`, `#[cfg(test)]`, `#[tokio::test]` and `#[rstest]` subtrees, which matters here
because tests are inline — 1,150 test functions against 6,249 lines in dedicated test files, so
a path rule would make a PR that adds tests read as one that adds complexity. `cognitive.value`
is a space's own score rather than `sum`, which folds in every nested closure. And
`--cyclomatic-count-try` decides whether Rust's `?` counts as a branch.

**Not Sonar, and not on licence grounds.** Sonar defined cognitive complexity, their analyzers
implement it best, and their licence turns out not to block internal agentic use — SonarSource's
own MCP server hands analyzer output to third-party agents under byte-identical SSAL and carries
a clarification from their VP Legal that doing so is a Non-competitive Purpose. The reasons to
pass are simpler: the platform needs a server we do not want to run, the free self-hosted tier
analyzes the main branch only and so cannot gate a pull request at all, and the analyzers are
source-available rather than open source, where `bca` is MPL-2.0 and already on the `deny.toml`
allowlist. On our TypeScript `bca` tracks Sonar's own implementation at Spearman 0.951, and
Sonar's worst eight functions all land inside `bca`'s top twenty-five — immaterial for a
threshold gate.

Rejected after measurement: Clippy's `cognitive_complexity` runs on macro-expanded HIR, so
against this repo's 2,871 `t!()` sites it ranks by i18n density (Spearman 0.435 against a real
cognitive metric, sharing 9 of its top 50). lizard's Rust reader never counts match arms while
counting every `?` and `where`, and its TypeScript reader silently drops functions from any file
with an object key named `interface`. `scc` and `tokei` are per-file or carry no complexity
metric at all.

**One blind spot, and it is universal.** A body inside `macro_rules!` scores zero — in `bca`, in
`rust-code-analysis`, and in SonarSource's own analyzer. Macro bodies are 0.19% of this repo's
Rust lines, so it is disclosed rather than instrumented.

## Baseline

| scope | functions | total cognitive | p95 | p99 | max |    over 25 |
| ----- | --------: | --------------: | --: | --: | --: | ---------: |
| Rust  |    11,282 |          16,823 |   7 |  24 | 155 | 101 (0.9%) |
| TS/JS |     5,436 |           6,816 |   6 |  15 |  72 |  15 (0.3%) |

`> 25` is the actionable line: under 1% of functions, 116 repo-wide.

Worst ten, which is the standing pay-down list:

```
155  ipv6_set               projects/start-wrt/backend/ctrl/src/lan.rs:395
150  update                 shared-libs/crates/start-core/src/net/net_controller.rs:358
144  update_addresses       shared-libs/crates/start-core/src/net/host/mod.rs:141
135  update_profile_ips_…   projects/start-wrt/backend/ctrl/src/lan.rs:668
133  set                    projects/start-wrt/backend/ctrl/src/published_ports.rs:867
125  <anonymous>            shared-libs/crates/start-core/src/net/host/address.rs:472
 97  rebase                 shared-libs/crates/patch-db/core/src/patch.rs:105
 80  up                     shared-libs/crates/start-core/src/version/v0_4_0_alpha_20.rs:37
 76  set                    projects/start-wrt/backend/ctrl/src/profiles.rs:1027
 72  <anonymous>            projects/start-sdk/lib/version/VersionGraph.ts:98
```

The closure at `net/host/address.rs:472` outranks every named function but four.

## The tooling

`make complexity` (totals and the worst 25), `make complexity-top` (the same list, any length),
`make complexity-diff` (this branch against its merge-base) and `make complexity-record` (append
a row to the history log). No build: the census is a tree-sitter pass, 0.35 s for the whole repo,
and a full delta including the base extraction is about 3 s. It lives in `build/complexity/`,
matching `build/fmt/`.

Real output, for the portmap gateway PR:

```
Complexity vs 93d0c3cc4e
  functions     16017 ->   16856   +839
  cognitive     28504 ->   29505   +1001
  sloc         190229 ->  199347   +9118
  fns over 25     156 ->     158   +2

  new functions over cognitive 10 (29 of 599 new):
    cog   53  try_apply  .../net/port_map/client.rs:677
    cog   33  desired_port_maps  .../net/vhost.rs:592

  existing functions made more complex (108):
    cog 36 -> 62  poll_ip_info  .../net/gateway.rs:2340  <-- already over 25
    cog 24 -> 39  gc_policy_routing  .../net/gateway.rs:1487  <-- now over 25

  simplified (52):
    cog 212 -> 165  update  .../net/net_controller.rs:358
    cog  42 ->   2  apply   .../net/port_map/client.rs:658
```

The aggregate is unremarkable, and that is the point. Measured across 30 real PRs, the
threshold _counts_ — how many functions sit over a line — moved on 0 of 30, and total
cognitive tracks the LOC delta closely enough (Spearman 0.82) that it is mostly line count
wearing a hat. Neither is a budget worth defending.

What a reviewer wants is the third block: a function already at 36 went to 62, and another
crossed 25. That list moves on nearly every PR, is not derivable from the diff size, and is
the thing a human would have flagged by hand. The report is per-function for that reason, and
it credits the 52 functions this PR simplified for the same one.

## Three numbers that are not line count

Per-function complexity is the headline, but it correlates with diff size. Three cheap
measures carry signal that LOC does not, and all three are things an agent inflates without
noticing:

- **Duplication.** 10.67% of lines repo-wide by union-of-ranges (`jscpd`, one 4.4 MB binary,
  under a second). `rpc-toolkit` is worst at 39.2%. Copy-paste is the most common way an agent
  adds volume without adding capability.
- **Unused dependencies.** `cargo-machete` finds **28 unused direct Cargo dependencies** today,
  in 0.26 s and without compiling. Cargo.lock sits at 994 crates.
- **Public surface.** The count of exported items — 923 `pub fn` in start-core alone. Widening
  an API is a permanent cost that no per-function metric registers.

None of these are gated either; they are reported because they move independently of diff size.

## No gate

Nothing here fails a build, blocks a merge, or caps a number. That is deliberate, and it is what
makes the rest safe: a metric with a reward attached gets optimised, and every cheap way to
optimise this one makes the code worse. Splitting a clear function into six poorly-named pieces
lowers its cognitive score. Hiding a body in `macro_rules!` takes it to zero. Leaving a helper
inlined avoids a new function. None of those are improvements, and all of them are what a gate
would buy.

So the numbers are **context handed to whoever is making the change**, and the only thing asked
of them is that they look.

## What the report is for

`make complexity-diff` prints what a branch did against its merge-base: the totals, every
function it pushed higher, every one it simplified, what it added to a util module, and which
utilities a second subsystem now depends on. It is a mirror, not a score.

An unexpected rise is a **symptom**. It usually means one of a small number of things — a
function grew a branch it did not need, a helper was shredded rather than extracted, a special
case was threaded through a call chain instead of handled at the edge — and the per-function
lines are there so the author can tell which. The right response to a symptom is to look, and
then either fix the mess or explain why the number is wrong about it.

**A rise that the author stands behind should be defensible, and the report is built to let it
be defended.** Complexity that is intrinsic to a requirement is still complexity: a protocol
with nine message types has nine branches wherever it is handled, and no restructuring removes
them. Because the report names functions rather than totals, an author can point at the specific
function, say what the branches are, and be right. A number is evidence, not a verdict — and the
one thing that would make it a verdict is a gate.

## Tracking across changes

`build/complexity/history.tsv` holds one row per master commit: commit, date, functions,
cognitive, cyclomatic, sloc, and the count over 25. It is seeded with 28 sampled points from
history and appended by `make complexity-record`.

**Only master CI ever writes it**, which is what keeps it free. Simulating real merges over 60
code commits, a totals file that pull requests edit conflicts on **75.4%** of median-lifetime
branches; the same file written only after merge conflicts on none, because no branch ever
touches it.

Read the log for shape, not for precision. Step changes in it are usually imports rather than
growth — the jump from 16,188 to 22,032 on 2026-07-02 is start-wrt and start-cli arriving in the
monorepo, not a bad week.

## Utilities

A general-purpose utility has one call site the day it is written, so anything that charges for
a single-use function charges for the library we want, and collects in inlined helpers and
helpers bent to fit one caller. Nothing here charges for it.

Credit, such as it is, attaches to **adoption rather than creation**: a function is named in the
report when a subsystem that did not call it before starts to, and only once the total reaches
two. The first caller is the author; the second is where generality stops being a claim. That
ordering also means writing a second utility for a job an existing one already does earns
nothing, while calling the existing one does — without anyone being penalised for either.

Nothing mechanical catches a re-implemented utility. `jscpd` flags a renamed copy-paste at 45%
duplicated lines, but the same helper written afresh with a different signature registers zero
clones, because the duplication is semantic. Searching for an existing helper before adding one
remains a habit, not a check.

## Known limits

- **Angular templates are invisible.** 278 components hold their markup in inline `template:`
  backticks containing 831 control-flow constructs, and those sit inside string literals that no
  per-function metric sees. Sonar's analyzer has the same blind spot.
- **Macro bodies score zero**, in every tool including SonarSource's own. They are 0.19% of this
  repo's Rust.
- **The metric is a proxy.** Cognitive complexity tracks nesting and branching, which is most of
  what makes code hard to hold in the head, and none of what makes a name wrong, an abstraction
  leaky, or an interface badly cut.
- **`bca` is four months old and carried by one maintainer.** That is the trade for it being
  maintained at all; its parent's last release is January 2023 and Mozilla no longer uses it.

## What is left to decide

1. **Where the report reaches an agent.** A CI job can post the delta as a PR comment, or the
   convention can be that whoever opens the PR runs `make complexity-diff` and pastes it. The
   first is reliable and costs a job; the second costs nothing and is followed about 28% of the
   time, measured against the closest existing precedent in `AGENTS.md`.
2. **Whether master CI appends to the history log**, which is the only piece that needs write
   access to the repo.
3. **Whether any of this belongs in `AGENTS.md`.** It is 43 KB already, and a rule that gates
   nothing competes for attention with rules that do.
