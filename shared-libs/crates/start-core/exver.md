# exver — Extended Versioning

Extended semver supporting **downstream versioning** (wrapper updates independent of upstream) and **flavors** (package fork variants).

Two implementations exist:

- **Rust crate** (`exver`) — used in `core/`. Source: https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/exver
- **TypeScript** (`shared-libs/ts-modules/start-core/lib/exver/index.ts`, package `@start9labs/start-core`) — used in `sdk/` and `web/`

Both parse the same string format and agree on `satisfies` semantics.

## Version Format

An **ExtendedVersion** string looks like:

```
[#flavor:]upstream:downstream
```

- **upstream** — the original package version (semver-style: `1.2.3`, `1.2.3-beta.1`)
- **downstream** — the StartOS wrapper version (incremented independently)
- **flavor** — optional lowercase ASCII prefix for fork variants

Examples:

- `1.2.3:0` — upstream 1.2.3, first downstream release
- `1.2.3:2` — upstream 1.2.3, third downstream release
- `#bitcoin:21.0:1` — bitcoin flavor, upstream 21.0, downstream 1
- `1.0.0-rc.1:0` — upstream with prerelease tag

## Core Types

### `Version`

A semver-style version with arbitrary digit segments and optional prerelease.

**Rust:**

```rust
use exver::Version;

let v = Version::new([1, 2, 3], []);              // 1.2.3
let v = Version::new([1, 0], ["beta".into()]);     // 1.0-beta
let v: Version = "1.2.3".parse().unwrap();

v.number()      // &[1, 2, 3]
v.prerelease()  // &[]
```

**TypeScript:**

```typescript
const v = new Version([1, 2, 3], [])
const v = Version.parse('1.2.3')

v.number // number[]
v.prerelease // (string | number)[]
v.compare(other) // 'greater' | 'equal' | 'less'
v.compareForSort(other) // -1 | 0 | 1
```

Default: `0`

### `ExtendedVersion`

The primary version type. Wraps upstream + downstream `Version` plus an optional flavor.

**Rust:**

```rust
use exver::ExtendedVersion;

let ev = ExtendedVersion::new(
    Version::new([1, 2, 3], []),
    Version::default(),          // downstream = 0
);
let ev: ExtendedVersion = "1.2.3:0".parse().unwrap();

ev.flavor()      // Option<&str>
ev.upstream()    // &Version
ev.downstream()  // &Version

// Builder methods (consuming):
ev.with_flavor("bitcoin")
ev.without_flavor()
ev.map_upstream(|v| ...)
ev.map_downstream(|v| ...)
```

**TypeScript:**

```typescript
const ev = new ExtendedVersion(null, upstream, downstream)
const ev = ExtendedVersion.parse('1.2.3:0')
const ev = ExtendedVersion.parseEmver('1.2.3.4') // emver compat

ev.flavor // string | null
ev.upstream // Version
ev.downstream // Version

ev.compare(other) // 'greater' | 'equal' | 'less' | null
ev.equals(other) // boolean
ev.greaterThan(other) // boolean
ev.lessThan(other) // boolean
ev.incrementMajor() // new ExtendedVersion
ev.incrementMinor() // new ExtendedVersion
```

**Ordering:** Versions with different flavors are **not comparable** (`PartialOrd`/`compare` returns `None`/`null`).

Default: `0:0`

### `VersionString` (Rust only, StartOS wrapper)

Defined in `core/src/util/version.rs`. Caches the original string representation alongside the parsed `ExtendedVersion`. Used as the key type in registry version maps.

```rust
use crate::util::VersionString;

let vs: VersionString = "1.2.3:0".parse().unwrap();
let vs = VersionString::from(extended_version);

// Deref to ExtendedVersion:
vs.satisfies(&range);
vs.upstream();

// String access:
vs.as_str();           // &str
AsRef::<str>::as_ref(&vs);
```

`Ord` is implemented with a total ordering — versions with different flavors are ordered by flavor name (unflavored sorts last).

### `VersionRange`

A predicate over `ExtendedVersion`. Supports comparison operators, boolean logic, and flavor constraints.

**Rust:**

```rust
use exver::VersionRange;

// Constructors:
VersionRange::any()                          // matches everything
VersionRange::none()                         // matches nothing
VersionRange::exactly(ev)                    // = ev
VersionRange::anchor(GTE, ev)               // >= ev
VersionRange::caret(ev)                      // ^ev (compatible changes)
VersionRange::tilde(ev)                      // ~ev (patch-level changes)

// Combinators (smart — eagerly simplify):
VersionRange::and(a, b)                      // a && b
VersionRange::or(a, b)                       // a || b
VersionRange::not(a)                         // !a

// Parsing:
let r: VersionRange = ">=1.0.0:0".parse().unwrap();
let r: VersionRange = "^1.2.3:0".parse().unwrap();
let r: VersionRange = ">=1.0.0 <2.0.0".parse().unwrap();   // implicit AND
let r: VersionRange = ">=1.0.0 || >=2.0.0".parse().unwrap();
let r: VersionRange = "#bitcoin".parse().unwrap();          // flavor match
let r: VersionRange = "*".parse().unwrap();                 // any

// Monoid wrappers for folding:
AnyRange  // fold with or, empty = None
AllRange  // fold with and, empty = Any
```

**TypeScript:**

```typescript
// Constructors:
VersionRange.any()
VersionRange.none()
VersionRange.anchor('=', ev)
VersionRange.anchor('>=', ev)
VersionRange.anchor('^', ev)                 // ^ and ~ are first-class operators
VersionRange.anchor('~', ev)
VersionRange.flavor(null)                    // match unflavored versions
VersionRange.flavor("bitcoin")               // match #bitcoin versions

// Combinators — static (smart, variadic):
VersionRange.and(a, b, c, ...)
VersionRange.or(a, b, c, ...)

// Combinators — instance (not smart, just wrap):
range.and(other)
range.or(other)
range.not()

// Parsing:
VersionRange.parse(">=1.0.0:0")
VersionRange.parseEmver(">=1.2.3.4")        // emver compat

// Analysis (TS only):
range.normalize()                            // canonical form (see below)
range.satisfiable()                          // boolean
range.intersects(other)                      // boolean
```

**Checking satisfaction:**

```rust
// Rust:
version.satisfies(&range)  // bool
```

```typescript
// TypeScript:
version.satisfies(range) // boolean
range.satisfiedBy(version) // boolean (convenience)
```

Also available on `Version` (wraps in `ExtendedVersion` with downstream=0).

When no operator is specified in a range string, `^` (caret) is the default.

## Operators

| Syntax | Rust                       | TS     | Meaning                                     |
| ------ | -------------------------- | ------ | ------------------------------------------- |
| `=`    | `EQ`                       | `'='`  | Equal                                       |
| `!=`   | `NEQ`                      | `'!='` | Not equal                                   |
| `>`    | `GT`                       | `'>'`  | Greater than                                |
| `>=`   | `GTE`                      | `'>='` | Greater than or equal                       |
| `<`    | `LT`                       | `'<'`  | Less than                                   |
| `<=`   | `LTE`                      | `'<='` | Less than or equal                          |
| `^`    | expanded to `And(GTE, LT)` | `'^'`  | Compatible (first non-zero digit unchanged) |
| `~`    | expanded to `And(GTE, LT)` | `'~'`  | Patch-level (minor unchanged)               |

## Flavor Rules

- Versions with **different flavors** never satisfy comparison operators (except `!=`, which returns true)
- `VersionRange::Flavor(Some("bitcoin"))` matches only `#bitcoin:*` versions
- `VersionRange::Flavor(None)` matches only unflavored versions
- Flavor constraints compose with `and`/`or`/`not` like any other range

## Reduction and Normalization

### Rust: `reduce()` (shallow)

`VersionRange::reduce(self) -> Self` re-applies smart constructor rules to one level of the AST. Useful for simplifying a node that was constructed directly (e.g. deserialized) rather than through the smart constructors.

**Smart constructor rules applied by `and`, `or`, `not`, and `reduce`:**

`and`:

- `and(Any, b) → b`, `and(a, Any) → a`
- `and(None, _) → None`, `and(_, None) → None`

`or`:

- `or(Any, _) → Any`, `or(_, Any) → Any`
- `or(None, b) → b`, `or(a, None) → a`

`not`:

- `not(=v) → !=v`, `not(!=v) → =v`
- `not(and(a, b)) → or(not(a), not(b))` (De Morgan)
- `not(or(a, b)) → and(not(a), not(b))` (De Morgan)
- `not(not(a)) → a`
- `not(Any) → None`, `not(None) → Any`

### TypeScript: `normalize()` (deep, canonical)

`VersionRange.normalize(): VersionRange` in `shared-libs/ts-modules/start-core/lib/exver/index.ts` performs full normalization by converting the range AST into a canonical form. This is a deep operation that produces a semantically equivalent but simplified range.

**How it works:**

1. **`tables()`** — Converts the VersionRange AST into truth tables (`VersionRangeTable`). Each table is a number line split at version boundary points, with boolean values for each segment indicating whether versions in that segment satisfy the range. Separate tables are maintained per flavor (and for flavor negations).

2. **`VersionRangeTable.zip(a, b, func)`** — Merges two tables by walking their boundary points in sorted order and applying a boolean function (`&&` or `||`) to combine segment values. Adjacent segments with the same boolean value are collapsed automatically.

3. **`VersionRangeTable.and/or/not`** — Table-level boolean operations. `and` computes the cross-product of flavor tables (since `#a && #b` for different flavors is unsatisfiable). `not` inverts all segment values.

4. **`VersionRangeTable.collapse()`** — Checks if a table is uniformly true or false across all flavors and segments. Returns `true`, `false`, or `null` (mixed).

5. **`VersionRangeTable.minterms()`** — Converts truth tables back into a VersionRange AST in [sum-of-products](https://en.wikipedia.org/wiki/Canonical_normal_form#Minterms) canonical form. Each `true` segment becomes a product term (conjunction of boundary constraints), and all terms are joined with `or`. Adjacent boundary points collapse into `=` anchors.

**Example:** `normalize` can simplify:

- `>=1.0.0:0 && <=1.0.0:0` → `=1.0.0:0`
- `>=2.0.0:0 || >=1.0.0:0` → `>=1.0.0:0`
- `!(!>=1.0.0:0)` → `>=1.0.0:0`

**Also exposes:**

- `satisfiable(): boolean` — returns `true` if there exists any version satisfying the range (checks if `collapse(tables())` is not `false`)
- `intersects(other): boolean` — returns `true` if `and(this, other)` is satisfiable

## API Differences Between Rust and TypeScript

|                             | Rust                                                                      | TypeScript                                                                              |
| --------------------------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| **`^` / `~`**               | Expanded at construction to `And(GTE, LT)`                                | First-class operator on `Anchor`                                                        |
| **`not()`**                 | Static, eagerly simplifies (De Morgan, double negation)                   | Instance method, just wraps                                                             |
| **`and()`/`or()`**          | Binary static                                                             | Both binary instance and variadic static                                                |
| **Normalization**           | `reduce()` — shallow, one AST level                                       | `normalize()` — deep canonical form via truth tables                                    |
| **Satisfiability**          | Not available                                                             | `satisfiable()` and `intersects(other)`                                                 |
| **ExtendedVersion helpers** | `with_flavor()`, `without_flavor()`, `map_upstream()`, `map_downstream()` | `incrementMajor()`, `incrementMinor()`, `greaterThan()`, `lessThan()`, `equals()`, etc. |
| **Monoid wrappers**         | `AnyRange` (fold with `or`) and `AllRange` (fold with `and`)              | Not present — use variadic static methods                                               |
| **`VersionString`**         | Wrapper caching parsed + string form                                      | Not present                                                                             |
| **Emver compat**            | `From<emver::Version>` for `ExtendedVersion`                              | `ExtendedVersion.parseEmver()`, `VersionRange.parseEmver()`                             |

## Serde

All types serialize/deserialize as strings (requires `serde` feature, enabled in StartOS):

```json
{
  "version": "1.2.3:0",
  "targetVersion": ">=1.0.0:0 <2.0.0:0",
  "sourceVersion": "^0.3.0:0"
}
```
