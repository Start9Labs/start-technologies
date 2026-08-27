# Complexity budget

A way to track complexity in this repo, and a protocol that makes an agent state and defend
what its change cost. Every number below was measured on `7e58f9d45`.

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

**Cognitive complexity per function**, via `rust-code-analysis` (tree-sitter, real Rust and
TypeScript grammars). Not cyclomatic complexity, and the difference is the whole argument.

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
census refuses to report if the parser reads under 98% of files or if over 0.5% of AST nodes
are parse errors, because grammar rot is otherwise silent.

Today every one of 530 Rust files yields metrics, and 240 of 2,553,851 AST nodes — 0.009% —
are parse errors. They cluster in six Rust files, on modern trait syntax the pinned grammar
predates: generic associated types (`type Extended<'ext> where Self: 'ext`) and `impl Trait`
in argument and return position. That is the shape grammar rot takes, and it is what the
second guard watches: fifty times the current rate still passes.

**The parser does not expand macros, and that is a hole.** Wrapping a body in `macro_rules!`
takes it from cognitive 15 to **0** and cyclomatic 11 to 1 — measured, identical logic. The
repo already carries `macro_rules!`, so the evasion would read as native. There is no fix
inside this tool; the report therefore prints the total number of lines inside macro bodies
(368 today) so that moving code there is at least visible. Note clippy has the mirror defect
in the other direction — it measures macro-_expanded_ HIR, so with 2,871 `t!()` i18n call
sites its ranking tracks i18n density rather than code, correlating with a real cognitive
metric at Spearman 0.435 and sharing 9 of its top 50.

## Why not an off-the-shelf tool

Mostly we should, and this RFC should have opened with that. Sonar defined cognitive
complexity, and the metric here is theirs: verified against this build, a sequence of like
operators costs 1 rather than 1 per operator, which is their distinctive rule.

**SonarQube is the wrong shape, though, and not because of the metric.** Complexity is a pure
function of the tree — it needs no server. Sonar runs one because it is a platform: history,
dashboard, quality profiles, PR decoration by webhook. The analysis itself already happens on
the runner. And the free self-hosted tier, Community Build, analyzes the **main branch only** —
no branch or pull-request analysis, no decoration — so self-hosting buys post-merge tracking and
no pre-merge gate. The tier that gates is the hosted one. Paying a SaaS to compute a number we
can compute in a second locally is the wrong trade for this repo.

**For TypeScript, Sonar's own implementation runs locally.** `eslint-plugin-sonarjs` is
SonarSource's ESLint plugin from their SonarJS repository, and `sonarjs/cognitive-complexity` is
the reference implementation of the metric. Measured here: 457 files in **1.4 s**, no server, no
`tsconfig`, no type information. It ranks this repo's TypeScript at Spearman **0.954** against
the census below and reports totals **27% lower**. Where the reference implementation runs
locally, use it — the TypeScript half of any gate should be this plugin, not a third-party
reimplementation.

**For Rust there is no local Sonar implementation**, and that is the whole reason anything is
vendored here. Of what exists: Clippy is official and local but its `cognitive_complexity`
measures macro-_expanded_ HIR, so against this repo's 2,871 `t!()` sites it ranks by i18n
density (Spearman 0.435, 9 of 50 top-50 shared); lizard's Rust reader never counts match arms
while counting every `?` and `where`; `scc` and `tokei` are per-file or have no complexity
metric at all. `rust-code-analysis` is Mozilla's, tree-sitter based, peer-reviewed in SoftwareX,
and the only local tool that computes cognitive complexity for Rust — with the caveat that its
last release is January 2023.

**Comparing the two implementations found a bug in this one.** Reading `cognitive.sum` per
space counts the file-level container as a function and folds every nested closure into its
parent. Agreement with the reference implementation was Spearman 0.822; taking each space's own
score instead — `sum` minus its children — moves it to **0.954** and drops the repo total by
33%. `list_conffiles` now scores 70, matching an independent measurement of the same function.
That is the argument for the vetted tool, made concrete: a reimplementation is wrong in ways you
only find by diffing it against the reference.

## Baseline## Baseline

| scope | functions | total cognitive | p95 | p99 | max |   over 25 |
| ----- | --------: | --------------: | --: | --: | --: | --------: |
| Rust  |    11,686 |          16,210 |   7 |  23 | 150 | 88 (0.8%) |
| TS    |     5,166 |           5,926 |   6 |  14 |  72 | 15 (0.3%) |

`> 25` is the actionable line: under 1% of functions, 103 repo-wide.

Worst ten, which is the standing pay-down list:

```
150  update                 shared-libs/crates/start-core/src/net/net_controller.rs:358
144  update_addresses       shared-libs/crates/start-core/src/net/host/mod.rs:141
135  update_profile_ips_…   projects/start-wrt/backend/ctrl/src/lan.rs:668
125  <anonymous>            shared-libs/crates/start-core/src/net/host/address.rs:472
118  ipv6_set               projects/start-wrt/backend/ctrl/src/lan.rs:395
 98  set                    projects/start-wrt/backend/ctrl/src/published_ports.rs:849
 97  rebase                 shared-libs/crates/patch-db/core/src/patch.rs:105
 80  up                     shared-libs/crates/start-core/src/version/v0_4_0_alpha_20.rs:37
 72  <anonymous>            projects/start-sdk/lib/version/VersionGraph.ts:98
 70  list_conffiles         projects/start-wrt/backend/ctrl/src/setup.rs:249
```

The closure at `net/host/address.rs:472` outranks every named function but four.

## The tooling

`make complexity` (totals + worst 25), `make complexity-top` (pay-down list), and
`make complexity-diff` (this branch against its merge-base). No build; the census is a
tree-sitter pass, 0.5 s for the whole repo, and a full delta including the base checkout is
about 3 s. It lives in `build/complexity/`, matching `build/fmt/`.

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

These are reported alongside the delta rather than gated, at least to start.

## Tracking over time, without the rebase tax

`complexity-diff` computes its base at run time, so nothing is committed and nothing conflicts.
That gives review-time confrontation but no history. For history, the instinct is to commit a
small aggregate — and simulating real merges with `git merge-file` over 60 real code commits
says that is the worst option available:

| committed artifact                            | conflict rate on a median-lifetime PR |
| --------------------------------------------- | ------------------------------------: |
| one-line totals                               |                                 75.4% |
| 15-line per-scope totals                      |                                 66.7% |
| sorted per-function watchlist, over threshold |                             **10.5%** |

Every PR rewrites the same totals line; PRs rarely touch the same region of a sorted 234-line
list. Splitting per-scope buys exactly nothing — 48 conflicts either way, because every
conflict is intra-scope. So if we want a committed record, it should be the watchlist of
functions over the threshold, regenerated by the existing drift idiom, not a scoreboard.

## The protocol

The confrontation belongs in the PR body, because that is the artifact an agent always
produces: helix-nine wrote a body on 188 of 188 merged PRs with a median of 2,850 characters,
while humans left 28 of 198 and 26 of 75 empty.

Documentation alone is not enough. The closest measurable precedent is the "Label every PR"
rule — inlined, bolded, with the literal command — which landed eight days before this
snapshot and runs at 60% agent compliance in that window and 28% across the last 90 merged
PRs. So the rule is paired with a check.

`make complexity-diff`'s output goes under a `## Complexity` heading, followed by three
questions. CI recomputes the census from `base.sha..head.sha` and fails when the section is
missing, unanswered, or carries numbers that disagree. **Prose can be bluffed; a number CI
recomputes cannot.** That is the load-bearing part of the design — the agent cannot write the
block without having run the tool.

The three questions, chosen because a weak answer is visible to a human:

- the simplest alternative considered, and what breaks if we take it
- which existing helper was checked before adding a new one, by file
- for any function pushed over 25, why the branching is intrinsic to the requirement
- for each new function with exactly one call site, why it earns its own name

The last one is the only question in the set an author cannot bluff, because the census counts
the call sites itself. It also targets the most common real defect in agent-written code — the
helper extracted for a reuse that never arrives. The portmap PR added 83 of them.

**The last two questions are asked only when the census reports them.** Requiring all four on
every PR would put a four-line section on the 61% of source PRs whose delta is near zero
against the 19% that are substantial, which is a rubber-stamping machine rather than a gate.

27% of merged PR bodies already volunteer a rejected alternative, so the hardest of the three
is culturally native here rather than an imposition.

## What this deliberately does not do

**It does not ratchet.** Features cost complexity: summed cognitive rises on 42 of 60 real code
commits and falls on 5, so a hard ratchet would block two commits in three and be suspended
during the first release crunch, never to return. The gate asks for a number and a reason,
not for the number to stay flat.

**It reports cyclomatic next to cognitive, because cognitive alone rewards shredding.** This
was the first version's mistake. Cognitive complexity penalises nesting superlinearly, so
pulling nested blocks out into separate functions lowers the total however bad the split is.
Measured on a deliberately worse six-way split that threads loop state through `&mut`
parameters, total cognitive falls **26 → 9** while total cyclomatic rises **10 → 16**.
Cognitive ranks a single function; cyclomatic is close to additive and so survives relocation.
The report prints both and says so outright when cognitive falls while cyclomatic rises —
branches were moved, not removed.

**It does not count tests**, so there is never a reason to thin one.

Known limits, stated rather than hidden: Angular templates are invisible — 278 components use
inline `template:` backticks holding 831 control-flow constructs, and those sit inside string
literals that no per-function metric sees. `rust-code-analysis`'s last release is v0.0.25 from
January 2023, though its grammar parses 100% of this repo today and the coverage assertion
above is what catches it if that changes. And the length check on the prose answers catches
laziness, not sophistry: an LLM writes a fluent post-hoc justification easily, so do not sell
the gate as catching bad reasoning. It catches an unexamined change. Only the numbers and the
call-site count are self-verifying.

## Open questions

1. **Advisory or required?** There is no `required_status_checks` rule on `master` today, so a
   red check does not literally block; the approval does. Nothing has merged red in the last
   30 PRs, so an advisory gate is honored in practice. Making it required is a one-line
   ruleset change — worth doing, or not yet?
2. **Should the gate apply to humans, or only to agent-authored PRs?** As written it applies to
   everyone, which is the honest version, but it lands hardest on the author who already writes
   the longest PR bodies.
3. **Is `> 25` the right line?** It flags 1% of functions today. `> 15` would flag 2.4%.
4. **Pay-down.** 75% of the twenty densest files were touched in the last 200 commits, so
   "pay some down when your change already puts you in one of these files" would fire often.
   Standing rule, or left to judgement?
