# AGENTS.md — exver

A semver extension that tracks an upstream version and a downstream (packaging) version
separately, with an optional flavor prefix. First-party crate in the start-technologies monorepo at
`shared-libs/crates/exver` (Cargo package `exver`). `CLAUDE.md`
is a one-line `@AGENTS.md` import. See ARCHITECTURE.md for how it works and CONTRIBUTING.md for
the workflow.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/lib.rs` — crate root. Re-exports `exver::*` and `emver`; gates the test module.
- `src/exver.rs` — all core types and logic: `Version`, `ExtendedVersion`, `VersionRange`,
  `ParseError`, `PreReleaseSegment`, the `Operator` constants (`EQ`/`NEQ`/`GT`/`GTE`/`LT`/`LTE`),
  the `AnyRange`/`AllRange` monoid wrappers, the Pest `Grammar`, `FromStr`/`Display`/`Serialize`
  impls, comparison/`satisfies` logic, and the private `sat` satisfiability module.
- `src/grammar.pest` — Pest grammar for `Version`, `ExtendedVersion`, and `VersionRange`.
- `src/test.rs` — proptest property tests for the `VersionRange` algebraic laws; `#[cfg(test)]`.
- `proptest-regressions/` — proptest's persisted failing-case seeds.

## Build & test (run from the repo root)

```bash
cargo build -p exver
cargo test  -p exver
```

## Gotchas

- Flavor-aware ordering is partial. `ExtendedVersion::partial_cmp` returns `None` when flavors
  differ — only use `<`/`>`/`<=`/`>=` between same-flavor versions; cross-flavor logic lives in
  `satisfies`. There is no total `Ord` for `ExtendedVersion`.
- Parsing requires upstream and downstream. The string form is `#flavor:upstream:downstream` or
  `upstream:downstream`; the flavor is optional. Some range-context helpers default a missing
  downstream to `0`, but the `ExtendedVersion` grammar (`extended_version`) requires both.
- `VersionRange::satisfiable()` and `intersects()` go through the truth-table algorithm in the
  private `sat` module. They are exact (they catch contradictions the smart constructors can't
  fold away) but can be expensive on large, deeply nested ranges.
- Prefer the smart constructors `VersionRange::and`/`or`/`not` over the raw `And`/`Or`/`Not`
  variants — they fold identities/annihilators eagerly and keep the AST small.
- The grammar and the `FromStr` impls must stay in sync: any change to the version or range
  string format means editing both `src/grammar.pest` and the parser code in `src/exver.rs`.
- **This crate has no JavaScript surface — never send a TypeScript consumer here.** JS/TS callers
  use the independent TypeScript reimplementation of ExVer in `@start9labs/start-core`
  (`shared-libs/ts-modules/start-core/lib/exver/`), which the published `@start9labs/start-sdk`
  re-exports as `ExtendedVersion` / `Version` / `VersionRange`.
- **That reimplementation shares this crate's spec but none of its code, and nothing enforces
  that they agree.** A change to the version/range format or to ordering here must land in
  `shared-libs/ts-modules/start-core/lib/exver/` (`exver.pegjs` + `index.ts`) in the same change,
  or the registries and their clients will disagree about which version is newer.
