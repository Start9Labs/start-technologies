# exver Architecture

exver extends SemVer with an independent downstream (packaging) version and an optional flavor
prefix, so a package distributor can track the upstream project's version and their own wrapper
revision separately. It also provides a `VersionRange` algebra with exact satisfiability checking.

## Place in the monorepo

- Path: `shared-libs/crates/exver`.
- Cargo package: `exver` (the directory name and package name match).
- Crate-type: `["cdylib", "rlib"]` — an rlib for Rust consumers and a cdylib for the WebAssembly
  build published to npm as `@start9labs/exver`.
- Consumers: `start-core` (`shared-libs/crates/start-core`, `exver = { path = "../exver",
features = ["serde"] }`) uses `Version`, `ExtendedVersion`, and `VersionRange` throughout
  package management, the registry, dependency resolution, and manifest handling.
- First-party: a direct path dependency, not a registry crate and not under any `[patch]`.

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

## WebAssembly surface

`src/wasm.rs` (feature `wasm`) exposes three functions via `wasm-bindgen`: `flavor(version)`,
`compare(lhs, rhs)` (returns `-1`/`0`/`1`, or `null` when flavors are incomparable), and
`satisfies(version, range)`. These are the JavaScript API published as `@start9labs/exver`; the
`exver.d.ts` sidecar is the hand-maintained type declaration shipped with that package.

## Further reading

- README.md — what the crate is and how to use the public API (format, ranges, laws).
- CONTRIBUTING.md — toolchain, build/test, and PR conventions.
- AGENTS.md — agent-facing rules and gotchas (CLAUDE.md is a one-line `@AGENTS.md` import).
