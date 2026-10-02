# exver Architecture

exver extends SemVer with an independent downstream (packaging) version and an optional flavor
prefix, so a package distributor can track the upstream project's version and their own wrapper
revision separately. It also provides a `VersionRange` algebra with exact satisfiability checking.

## Place in the monorepo

- Path: `shared-libs/crates/exver`.
- Cargo package: `exver` (the directory name and package name match).
- Consumers: `start-core` (`shared-libs/crates/start-core`, `exver = { path = "../exver",
features = ["serde"] }`) uses `Version`, `ExtendedVersion`, and `VersionRange` throughout
  package management, the registry, dependency resolution, and manifest handling.
- First-party: a direct path dependency, not a registry crate and not under any `[patch]`.
- Rust only. The TypeScript ExVer in `@start9labs/start-core`
  (`shared-libs/ts-modules/start-core/lib/exver/`) is an independent reimplementation of this
  spec, not a binding to this crate — see § Relationship to the TypeScript implementation.

## Core types

All public types live in `src/exver.rs` (re-exported from `src/lib.rs`).

- `Version` — an arbitrary-length list of numeric components plus optional prerelease segments
  (`PreReleaseSegment`). Follows SemVer ordering semantics but allows any number of digits.
- `ExtendedVersion` — an optional flavor, an `upstream` `Version`, and a `downstream` `Version`.
  Downstream is strictly less significant than upstream. Displayed/parsed as
  `#flavor:upstream:downstream`.
- `VersionRange` — a set of `ExtendedVersion`s, either an anchor (`Operator` + version) or a
  logical combination (`And`, `Or`, `Not`) with `Any`/`None` identities. Smart constructors
  `and`/`or`/`not`/`anchor`/`caret`/`tilde`/`exactly` fold identities and annihilators eagerly.
- `Operator` — `Invertable<Ordering>` (`Result<Ordering, Ordering>`); the public constants `EQ`,
  `NEQ`, `GT`, `GTE`, `LT`, `LTE` encode the six comparison anchors.
- `AnyRange` / `AllRange` — monoid wrappers (`Semigroup`/`Empty`/`Monoid` from `fp-core`) for
  folding an iterator of ranges with `or` or `and` respectively.
- `ParseError` — the error returned by every `FromStr` impl.

## Ordering and satisfaction

`ExtendedVersion` ordering is partial: `partial_cmp` compares upstream then downstream, but
returns `None` across different flavors, so flavors form incomparable lineages. The `satisfies`
predicate (on both `Version` and `ExtendedVersion`) is the library's single observer — it walks a
`VersionRange` and decides membership, handling the cross-flavor cases that bare comparison can't.

## Parsing

`src/grammar.pest` defines the Pest grammar; `pest_derive` generates the parser bound to the
`Grammar` type. The grammar covers `version`, `extended_version`, and the full `version_range`
syntax including `&&`/`||`/`!`/`*`, parenthesised sub-ranges, comparison operators, and the
SemVer `^`/`~` shorthands. Each public type's `FromStr` impl drives the grammar and builds the
corresponding type, so the grammar and the parser code must change together.

## Satisfiability (the `sat` module)

The private `sat` module backs `VersionRange::satisfiable()` and `intersects()`. Smart
constructors fold obvious identities/annihilators, but they cannot detect every emptiness or
contradiction (e.g. `>=2 && <1`). `sat` builds a truth-table over the relevant anchor points and
evaluates the boolean structure exactly. It is precise but can be expensive on large, deeply
nested ranges.

## Relationship to the TypeScript implementation

`@start9labs/start-core` implements this same spec independently in TypeScript
(`shared-libs/ts-modules/start-core/lib/exver/` — a peggy grammar plus `ExtendedVersion` /
`Version` / `VersionRange` classes), and `@start9labs/start-sdk` bundles and re-exports it. That is
what every JS/TS caller uses; this crate has no JavaScript surface of its own.

The two are separate code sharing one spec, and nothing enforces that they agree — no generated
bindings, no shared test vectors. `ExtendedVersion.compare` returning
`'greater' | 'equal' | 'less' | null` mirrors `partial_cmp`'s cross-flavor `None` by hand. So a
change to the version/range format or to ordering here has to land there too, or the registries and
their clients will disagree about which version is newer.

## Further reading

- README.md — what the crate is and how to use the public API (format, ranges, laws).
- AGENTS.md — toolchain, build/test, and PR conventions.
