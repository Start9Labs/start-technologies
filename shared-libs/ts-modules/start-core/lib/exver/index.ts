import { DeepMap } from 'deep-equality-data-structures'
import * as P from './exver'

/**
 * Compile-time utility type that validates a version string literal conforms to semver format.
 *
 * Resolves to `unknown` if valid, `never` if invalid. Used with {@link testTypeVersion}.
 *
 * @example
 * ```ts
 * type Valid = ValidateVersion<"1.2.3">     // unknown (valid)
 * type Invalid = ValidateVersion<"-3">      // never (invalid)
 * ```
 */
// prettier-ignore
export type ValidateVersion<T extends String> =
T extends `-${infer A}` ? never  :
T extends `${infer A}-${string}` ? ValidateVersion<A> :
  T extends `${bigint}` ? unknown :
  T extends `${bigint}.${infer A}` ? ValidateVersion<A> :
  never

/**
 * Compile-time utility type that validates an extended version string literal.
 *
 * Extended versions have the format `upstream:downstream` or `#flavor:upstream:downstream`.
 *
 * @example
 * ```ts
 * type Valid = ValidateExVer<"1.2.3:0">           // valid
 * type Flavored = ValidateExVer<"#bitcoin:1.0:0">  // valid
 * type Bad = ValidateExVer<"1.2-3">                // never (invalid)
 * ```
 */
// prettier-ignore
export type ValidateExVer<T extends string> =
  T extends `#${string}:${infer A}:${infer B}` ? ValidateVersion<A> & ValidateVersion<B> :
  T extends `${infer A}:${infer B}` ? ValidateVersion<A> & ValidateVersion<B> :
  never

/**
 * Validates a tuple of extended version string literals at compile time.
 *
 * @example
 * ```ts
 * type Valid = ValidateExVers<["1.0:0", "2.0:0"]>  // valid
 * ```
 */
// prettier-ignore
export type ValidateExVers<T> =
  T extends [] ? unknown[] :
  T extends [infer A, ...infer B] ? ValidateExVer<A & string> & ValidateExVers<B> :
  never[]

/**
 * Validates a `[#flavor:]upstream[:downstream]` version spec (the downstream
 * segment and flavor are optional, e.g. `29`, `29.3`, `29.3:0`, `#knots:29.3:0`),
 * as well as a bare `#flavor` atom. Each numeric segment is checked via
 * {@link ValidateVersion}.
 */
// prettier-ignore
type ValidateVersionSpec<T extends string> =
  T extends `#${string}:${infer A}:${infer B}` ? ValidateVersion<A> & ValidateVersion<B> :
  T extends `#${string}:${infer A}`            ? ValidateVersion<A> :
  T extends `#${string}`                       ? unknown :
  T extends `${infer A}:${infer B}`            ? ValidateVersion<A> & ValidateVersion<B> :
  ValidateVersion<T>

/** Strips leading spaces. */
type RangeTrimL<S extends string> = S extends ` ${infer R}` ? RangeTrimL<R> : S

/** A character that terminates a version-spec token. */
type RangeDelim = ' ' | '&' | '|' | ')' | '('

/** Reads a maximal version-spec token, returning `[token, rest]`. */
// prettier-ignore
type RangeReadSpec<S extends string, Acc extends string = ''> =
  S extends `${infer C}${infer R}`
    ? C extends RangeDelim ? [Acc, S] : RangeReadSpec<R, `${Acc}${C}`>
    : [Acc, '']

/** Validates one anchor (its version spec) then continues with the remainder. */
// prettier-ignore
type RangeValidateSpecToken<S extends string> =
  RangeReadSpec<S> extends [infer Tok extends string, infer Rest extends string]
    ? [ValidateVersionSpec<Tok>] extends [never] ? never : ValidateRangeExpr<Rest>
    : never

/** Strips an optional leading comparison operator, then validates the spec. */
// prettier-ignore
type RangeValidateAtom<S extends string> =
  S extends `>=${infer R}` ? RangeValidateSpecToken<R> :
  S extends `<=${infer R}` ? RangeValidateSpecToken<R> :
  S extends `>${infer R}`  ? RangeValidateSpecToken<R> :
  S extends `<${infer R}`  ? RangeValidateSpecToken<R> :
  S extends `=${infer R}`  ? RangeValidateSpecToken<R> :
  S extends `^${infer R}`  ? RangeValidateSpecToken<R> :
  S extends `~${infer R}`  ? RangeValidateSpecToken<R> :
  RangeValidateSpecToken<S>

/**
 * Recursively consumes a version-range expression, validating every version
 * atom it encounters. Structural tokens (`&&`, `||`, parens, `!`, `*`) are
 * skipped — full grammar structure (paren balance, operator placement) is left
 * to runtime, but a malformed version anywhere in the expression is rejected.
 */
// prettier-ignore
type ValidateRangeExpr<S extends string> =
  RangeTrimL<S> extends ''             ? unknown :
  RangeTrimL<S> extends `&&${infer R}` ? ValidateRangeExpr<R> :
  RangeTrimL<S> extends `||${infer R}` ? ValidateRangeExpr<R> :
  RangeTrimL<S> extends `(${infer R}`  ? ValidateRangeExpr<R> :
  RangeTrimL<S> extends `)${infer R}`  ? ValidateRangeExpr<R> :
  RangeTrimL<S> extends `!${infer R}`  ? ValidateRangeExpr<R> :
  RangeTrimL<S> extends `*${infer R}`  ? ValidateRangeExpr<R> :
  RangeValidateAtom<RangeTrimL<S>>

/**
 * Compile-time validator for an exver version **range** string literal — the
 * full grammar (comparison / caret / tilde anchors, `&&` / `||`, parentheses,
 * negation, `#flavor` atoms, `*`). Resolves to `unknown` if valid, `never` if
 * any version atom is malformed. A non-literal `string` is accepted (nothing to
 * check at compile time); structural-only errors are deferred to runtime.
 *
 * @example
 * ```ts
 * type A = ValidateExVerRange<"^29">                     // valid
 * type B = ValidateExVerRange<">=29:0 && <30:0">         // valid
 * type C = ValidateExVerRange<"(>=1:0 && <2:0) || >=3:0"> // valid
 * type D = ValidateExVerRange<">=2.f">                   // never (bad version)
 * ```
 */
export type ValidateExVerRange<T extends string> = string extends T
  ? unknown
  : ValidateRangeExpr<T>

type Anchor = {
  type: 'Anchor'
  operator: P.CmpOp
  version: ExtendedVersion
}

type And = {
  type: 'And'
  left: VersionRange
  right: VersionRange
}

type Or = {
  type: 'Or'
  left: VersionRange
  right: VersionRange
}

type Not = {
  type: 'Not'
  value: VersionRange
}

type Flavor = {
  type: 'Flavor'
  flavor: string | null
}

type FlavorNot = {
  type: 'FlavorNot'
  flavors: Set<string | null>
}

type FlavorAtom = Flavor | FlavorNot

/**
 * Splits a number line of versions in half, so that every possible semver is either to the left or right.
 * The `side` field handles inclusively.
 *
 * # Example
 * Consider the version `1.2.3`. For side=-1 the version point is like `1.2.2.999*.999*.**` (that is, 1.2.3.0.0.** is greater) and
 * for side=+1 the point is like `1.2.3.0.0.**.1` (that is, 1.2.3.0.0.** is less).
 */
type VersionRangePoint = {
  upstream: Version
  downstream: Version
  side: -1 | 1
}

function compareVersionRangePoints(
  a: VersionRangePoint,
  b: VersionRangePoint,
): -1 | 0 | 1 {
  let up = a.upstream.compareForSort(b.upstream)
  if (up != 0) {
    return up
  }
  let down = a.downstream.compareForSort(b.downstream)
  if (down != 0) {
    return down
  }
  if (a.side < b.side) {
    return -1
  } else if (a.side > b.side) {
    return 1
  } else {
    return 0
  }
}

function adjacentVersionRangePoints(
  a: VersionRangePoint,
  b: VersionRangePoint,
): boolean {
  let up = a.upstream.compareForSort(b.upstream)
  if (up != 0) {
    return false
  }
  let down = a.downstream.compareForSort(b.downstream)
  if (down != 0) {
    return false
  }
  return a.side == -1 && b.side == 1
}

function flavorAnd(a: FlavorAtom, b: FlavorAtom): FlavorAtom | null {
  if (a.type == 'Flavor') {
    if (b.type == 'Flavor') {
      if (a.flavor == b.flavor) {
        return a
      } else {
        return null
      }
    } else {
      if (b.flavors.has(a.flavor)) {
        return null
      } else {
        return a
      }
    }
  } else {
    if (b.type == 'Flavor') {
      if (a.flavors.has(b.flavor)) {
        return null
      } else {
        return b
      }
    } else {
      // TODO: use Set.union if targeting esnext or later
      return {
        type: 'FlavorNot',
        flavors: new Set([...a.flavors, ...b.flavors]),
      }
    }
  }
}

/**
 * Truth tables for version numbers and flavors. For each flavor we need a separate table, which
 * is quite straightforward. But in order to exhaustively enumerate the boolean values of every
 * combination of flavors and versions we also need tables for flavor negations.
 */
type VersionRangeTables = DeepMap<FlavorAtom, VersionRangeTable> | boolean

/**
 * A truth table for version numbers. This is easiest to picture as a number line, cut up into
 * ranges of versions between version points.
 */
class VersionRangeTable {
  private constructor(
    protected points: Array<VersionRangePoint>,
    protected values: boolean[],
  ) {}

  static zip(
    a: VersionRangeTable,
    b: VersionRangeTable,
    func: (a: boolean, b: boolean) => boolean,
  ): VersionRangeTable {
    let c = new VersionRangeTable([], [])
    let i = 0
    let j = 0
    while (true) {
      let next = func(a.values[i], b.values[j])
      if (c.values.length > 0 && c.values[c.values.length - 1] == next) {
        // collapse automatically
        c.points.pop()
      } else {
        c.values.push(next)
      }

      // which point do we step over?
      if (i == a.points.length) {
        if (j == b.points.length) {
          // just added the last segment, no point to jump over
          return c
        } else {
          // i has reach the end, step over j
          c.points.push(b.points[j])
          j += 1
        }
      } else {
        if (j == b.points.length) {
          // j has reached the end, step over i
          c.points.push(a.points[i])
          i += 1
        } else {
          // depends on which of the next two points is lower
          switch (compareVersionRangePoints(a.points[i], b.points[j])) {
            case -1:
              // i is the lower point
              c.points.push(a.points[i])
              i += 1
              break
            case 1:
              // j is the lower point
              c.points.push(b.points[j])
              j += 1
              break
            default:
              // step over both
              c.points.push(a.points[i])
              i += 1
              j += 1
              break
          }
        }
      }
    }
  }

  /**
   * Creates a version table which is `true` for the given flavor, and `false` for any other flavor.
   */
  static eqFlavor(flavor: string | null): VersionRangeTables {
    return new DeepMap([
      [
        { type: 'Flavor', flavor } as FlavorAtom,
        new VersionRangeTable([], [true]),
      ],
      // make sure the truth table is exhaustive, or `not` will not work properly.
      [
        { type: 'FlavorNot', flavors: new Set([flavor]) } as FlavorAtom,
        new VersionRangeTable([], [false]),
      ],
    ])
  }

  /**
   * Creates a version table with exactly two ranges (to the left and right of the given point) and with `false` for any other flavor.
   * This is easiest to understand by looking at `VersionRange.tables`.
   */
  static cmpPoint(
    flavor: string | null,
    point: VersionRangePoint,
    left: boolean,
    right: boolean,
  ): VersionRangeTables {
    return new DeepMap([
      [
        { type: 'Flavor', flavor } as FlavorAtom,
        new VersionRangeTable([point], [left, right]),
      ],
      // make sure the truth table is exhaustive, or `not` will not work properly.
      [
        { type: 'FlavorNot', flavors: new Set([flavor]) } as FlavorAtom,
        new VersionRangeTable([], [false]),
      ],
    ])
  }

  /**
   * Helper for `cmpPoint`.
   */
  static cmp(
    version: ExtendedVersion,
    side: -1 | 1,
    left: boolean,
    right: boolean,
  ): VersionRangeTables {
    return VersionRangeTable.cmpPoint(
      version.flavor,
      { upstream: version.upstream, downstream: version.downstream, side },
      left,
      right,
    )
  }

  static not(tables: VersionRangeTables) {
    if (tables === true || tables === false) {
      return !tables
    }
    // because tables are always exhaustive, we can simply invert each range
    for (let [f, t] of tables) {
      for (let i = 0; i < t.values.length; i++) {
        t.values[i] = !t.values[i]
      }
    }
    return tables
  }

  static and(
    a_tables: VersionRangeTables,
    b_tables: VersionRangeTables,
  ): VersionRangeTables {
    if (a_tables === true) {
      return b_tables
    }
    if (b_tables === true) {
      return a_tables
    }
    if (a_tables === false || b_tables == false) {
      return false
    }
    let c_tables: VersionRangeTables = true
    for (let [f_a, a] of a_tables) {
      for (let [f_b, b] of b_tables) {
        let flavor = flavorAnd(f_a, f_b)
        if (flavor == null) {
          continue
        }
        let c = VersionRangeTable.zip(a, b, (a, b) => a && b)
        if (c_tables === true) {
          c_tables = new DeepMap()
        }
        let prev_c = c_tables.get(flavor)
        if (prev_c == null) {
          c_tables.set(flavor, c)
        } else {
          c_tables.set(
            flavor,
            VersionRangeTable.zip(c, prev_c, (a, b) => a || b),
          )
        }
      }
    }
    return c_tables
  }

  static or(...in_tables: VersionRangeTables[]): VersionRangeTables {
    let out_tables: VersionRangeTables = false
    for (let tables of in_tables) {
      if (tables === false) {
        continue
      }
      if (tables === true) {
        return true
      }
      if (out_tables === false) {
        out_tables = new DeepMap()
      }
      for (let [flavor, table] of tables) {
        let prev = out_tables.get(flavor)
        if (prev == null) {
          out_tables.set(flavor, table)
        } else {
          out_tables.set(
            flavor,
            VersionRangeTable.zip(table, prev, (a, b) => a || b),
          )
        }
      }
    }
    return out_tables
  }

  /**
   * If this is true for all versions or false for all versions, returen that value. Otherwise return null.
   */
  static collapse(tables: VersionRangeTables): boolean | null {
    if (tables === true || tables === false) {
      return tables
    } else {
      let found = null
      for (let table of tables.values()) {
        for (let x of table.values) {
          if (found == null) {
            found = x
          } else if (found != x) {
            return null
          }
        }
      }
      return found
    }
  }

  /**
   * Expresses this truth table as a series of version range operators.
   * https://en.wikipedia.org/wiki/Canonical_normal_form#Minterms
   */
  static minterms(tables: VersionRangeTables): VersionRange {
    let collapse = VersionRangeTable.collapse(tables)
    if (tables === true || collapse === true) {
      return VersionRange.any()
    }
    if (tables == false || collapse === false) {
      return VersionRange.none()
    }
    let sum_terms: VersionRange[] = []
    for (let [flavor, table] of tables) {
      let cmp_flavor = null
      if (flavor.type == 'Flavor') {
        cmp_flavor = flavor.flavor
      }
      for (let i = 0; i < table.values.length; i++) {
        let term: VersionRange[] = []
        if (!table.values[i]) {
          continue
        }

        if (flavor.type == 'FlavorNot') {
          for (let not_flavor of flavor.flavors) {
            term.push(VersionRange.flavor(not_flavor).not())
          }
        }

        let p = null
        let q = null
        if (i > 0) {
          p = table.points[i - 1]
        }
        if (i < table.points.length) {
          q = table.points[i]
        }

        if (p != null && q != null && adjacentVersionRangePoints(p, q)) {
          term.push(
            VersionRange.anchor(
              '=',
              new ExtendedVersion(cmp_flavor, p.upstream, p.downstream),
            ),
          )
        } else {
          if (p != null && p.side < 0) {
            term.push(
              VersionRange.anchor(
                '>=',
                new ExtendedVersion(cmp_flavor, p.upstream, p.downstream),
              ),
            )
          }
          if (p != null && p.side >= 0) {
            term.push(
              VersionRange.anchor(
                '>',
                new ExtendedVersion(cmp_flavor, p.upstream, p.downstream),
              ),
            )
          }
          if (q != null && q.side < 0) {
            term.push(
              VersionRange.anchor(
                '<',
                new ExtendedVersion(cmp_flavor, q.upstream, q.downstream),
              ),
            )
          }
          if (q != null && q.side >= 0) {
            term.push(
              VersionRange.anchor(
                '<=',
                new ExtendedVersion(cmp_flavor, q.upstream, q.downstream),
              ),
            )
          }
        }

        if (term.length == 0) {
          term.push(VersionRange.flavor(cmp_flavor))
        }

        sum_terms.push(VersionRange.and(...term))
      }
    }
    return VersionRange.or(...sum_terms)
  }
}

/**
 * Represents a parsed version range expression used to match against {@link Version} or {@link ExtendedVersion} values.
 *
 * Version ranges support standard comparison operators (`=`, `>`, `<`, `>=`, `<=`, `!=`),
 * caret (`^`) and tilde (`~`) ranges, boolean logic (`&&`, `||`, `!`), and flavor matching (`#flavor`).
 *
 * @example
 * ```ts
 * const range = VersionRange.parse(">=1.0.0:0 && <2.0.0:0")
 * const version = ExtendedVersion.parse("1.5.0:0")
 * console.log(range.satisfiedBy(version)) // true
 *
 * // Combine ranges with boolean logic
 * const combined = VersionRange.and(
 *   VersionRange.parse(">=1.0:0"),
 *   VersionRange.parse("<3.0:0"),
 * )
 *
 * // Match a specific flavor
 * const flavored = VersionRange.parse("#bitcoin")
 * ```
 */
export class VersionRange {
  constructor(public atom: Anchor | And | Or | Not | P.Any | P.None | Flavor) {}

  toStringParens(parent: 'And' | 'Or' | 'Not') {
    let needs = true
    switch (this.atom.type) {
      case 'And':
      case 'Or':
        needs = parent != this.atom.type
        break
      case 'Anchor':
      case 'Any':
      case 'None':
        needs = parent == 'Not'
        break
      case 'Not':
      case 'Flavor':
        needs = false
        break
    }

    if (needs) {
      return '(' + this.toString() + ')'
    } else {
      return this.toString()
    }
  }

  /** Serializes this version range back to its canonical string representation. */
  toString(): string {
    switch (this.atom.type) {
      case 'Anchor':
        return `${this.atom.operator}${this.atom.version}`
      case 'And':
        return `${this.atom.left.toStringParens(this.atom.type)} && ${this.atom.right.toStringParens(this.atom.type)}`
      case 'Or':
        return `${this.atom.left.toStringParens(this.atom.type)} || ${this.atom.right.toStringParens(this.atom.type)}`
      case 'Not':
        return `!${this.atom.value.toStringParens(this.atom.type)}`
      case 'Flavor':
        return this.atom.flavor == null ? `#` : `#${this.atom.flavor}`
      case 'Any':
        return '*'
      case 'None':
        return '!'
    }
  }

  private static parseAtom(atom: P.VersionRangeAtom): VersionRange {
    switch (atom.type) {
      case 'Not':
        return new VersionRange({
          type: 'Not',
          value: VersionRange.parseAtom(atom.value),
        })
      case 'Parens':
        return VersionRange.parseRange(atom.expr)
      case 'Anchor':
        return VersionRange.anchor(
          atom.operator || '^',
          new ExtendedVersion(
            atom.version.flavor,
            new Version(
              atom.version.upstream.number,
              atom.version.upstream.prerelease,
            ),
            new Version(
              atom.version.downstream.number,
              atom.version.downstream.prerelease,
            ),
          ),
        )
      case 'Flavor':
        return VersionRange.flavor(atom.flavor)
      default:
        return new VersionRange(atom)
    }
  }

  private static parseRange(range: P.VersionRange): VersionRange {
    let result = VersionRange.parseAtom(range[0])
    for (const next of range[1]) {
      switch (next[1]?.[0]) {
        case '||':
          result = new VersionRange({
            type: 'Or',
            left: result,
            right: VersionRange.parseAtom(next[2]),
          })
          break
        case '&&':
        default:
          result = new VersionRange({
            type: 'And',
            left: result,
            right: VersionRange.parseAtom(next[2]),
          })
          break
      }
    }
    return result
  }

  /**
   * Parses a version range string into a `VersionRange`.
   *
   * @param range - A version range expression, e.g. `">=1.0.0:0 && <2.0.0:0"`, `"^1.2:0"`, `"*"`
   * @returns The parsed `VersionRange`
   * @throws If the string is not a valid version range expression
   */
  static parse(range: string): VersionRange {
    return VersionRange.parseRange(
      P.parse(range, { startRule: 'VersionRange' }),
    )
  }

  /**
   * Creates a version range from a comparison operator and an {@link ExtendedVersion}.
   *
   * @param operator - One of `"="`, `">"`, `"<"`, `">="`, `"<="`, `"!="`, `"^"`, `"~"`
   * @param version - The version to compare against
   */
  static anchor(operator: P.CmpOp, version: ExtendedVersion): VersionRange {
    switch (operator) {
      case '^':
        return VersionRange.and(
          VersionRange.anchor('>=', version),
          VersionRange.anchor('<', version.incrementMajor()),
        )
      case '~':
        return VersionRange.and(
          VersionRange.anchor('>=', version),
          VersionRange.anchor('<', version.incrementMinor()),
        )
      default:
        return new VersionRange({ type: 'Anchor', operator, version })
    }
  }

  /**
   * Creates a version range that matches only versions with the specified flavor.
   *
   * @param flavor - The flavor string to match, or `null` for the default (unflavored) variant
   */
  static flavor(flavor: string | null) {
    return new VersionRange({ type: 'Flavor', flavor })
  }

  /**
   * Parses a legacy "emver" format version range string.
   *
   * @param range - A version range in the legacy emver format
   * @returns The parsed `VersionRange`
   */
  static parseEmver(range: string): VersionRange {
    return VersionRange.parseRange(
      P.parse(range, { startRule: 'EmverVersionRange' }),
    )
  }

  /** Returns the intersection of this range with another (logical AND). */
  and(right: VersionRange) {
    return new VersionRange({ type: 'And', left: this, right })
  }

  /** Returns the union of this range with another (logical OR). */
  or(right: VersionRange) {
    return new VersionRange({ type: 'Or', left: this, right })
  }

  /** Returns the negation of this range (logical NOT). */
  not() {
    return new VersionRange({ type: 'Not', value: this })
  }

  /**
   * Returns the logical AND (intersection) of multiple version ranges.
   * Short-circuits on `none()` and skips `any()`.
   */
  static and(...xs: Array<VersionRange>) {
    let y = VersionRange.any()
    for (let x of xs) {
      if (x.atom.type == 'Any') {
        continue
      }
      if (x.atom.type == 'None') {
        return x
      }
      if (y.atom.type == 'Any') {
        y = x
      } else {
        y = new VersionRange({ type: 'And', left: y, right: x })
      }
    }
    return y
  }

  /**
   * Returns the logical OR (union) of multiple version ranges.
   * Short-circuits on `any()` and skips `none()`.
   */
  static or(...xs: Array<VersionRange>) {
    let y = VersionRange.none()
    for (let x of xs) {
      if (x.atom.type == 'None') {
        continue
      }
      if (x.atom.type == 'Any') {
        return x
      }
      if (y.atom.type == 'None') {
        y = x
      } else {
        y = new VersionRange({ type: 'Or', left: y, right: x })
      }
    }
    return y
  }

  /** Returns a version range that matches all versions (wildcard `*`). */
  static any() {
    return new VersionRange({ type: 'Any' })
  }

  /** Returns a version range that matches no versions (`!`). */
  static none() {
    return new VersionRange({ type: 'None' })
  }

  /**
   * Returns `true` if the given version satisfies this range.
   *
   * @param version - A {@link Version} or {@link ExtendedVersion} to test
   */
  satisfiedBy(version: Version | ExtendedVersion) {
    return version.satisfies(this)
  }

  /**
   * Whether the declared versions collectively satisfy the complete range.
   *
   * A negated range passes only when no declared version satisfies its complete operand.
   * `!=v` is evaluated as `!(=v)`. An empty release satisfies no range.
   */
  satisfiedByRelease(versions: ExtendedVersion[]): boolean {
    return this.releaseMatches(versions, false).some(Boolean)
  }

  private releaseMatches(
    versions: ExtendedVersion[],
    negated: boolean,
  ): boolean[] {
    if (this.atom.type === 'Not') {
      return this.atom.value.releaseMatches(versions, !negated)
    }
    if (this.atom.type === 'Anchor' && this.atom.operator === '!=') {
      return VersionRange.anchor('=', this.atom.version).releaseMatches(
        versions,
        !negated,
      )
    }
    if (negated) {
      const matched = versions.some(v => v.satisfies(this))
      return versions.map(() => !matched)
    }

    switch (this.atom.type) {
      case 'And': {
        const left = this.atom.left.releaseMatches(versions, false)
        const right = this.atom.right.releaseMatches(versions, false)
        return left.map((matched, i) => matched && right[i])
      }
      case 'Or': {
        const left = this.atom.left.releaseMatches(versions, false)
        const right = this.atom.right.releaseMatches(versions, false)
        return left.map((matched, i) => matched || right[i])
      }
      default:
        return versions.map(v => v.satisfies(this))
    }
  }

  tables(): VersionRangeTables {
    switch (this.atom.type) {
      case 'Anchor':
        switch (this.atom.operator) {
          case '=':
            // `=1.2.3` is equivalent to `>=1.2.3 && <=1.2.4 && #flavor`
            return VersionRangeTable.and(
              VersionRangeTable.cmp(this.atom.version, -1, false, true),
              VersionRangeTable.cmp(this.atom.version, 1, true, false),
            )
          case '>':
            return VersionRangeTable.cmp(this.atom.version, 1, false, true)
          case '<':
            return VersionRangeTable.cmp(this.atom.version, -1, true, false)
          case '>=':
            return VersionRangeTable.cmp(this.atom.version, -1, false, true)
          case '<=':
            return VersionRangeTable.cmp(this.atom.version, 1, true, false)
          case '!=':
            // `!=1.2.3` is equivalent to `!(>=1.2.3 && <=1.2.3 && #flavor)`
            // **not** equivalent to `(<1.2.3 || >1.2.3) && #flavor`
            return VersionRangeTable.not(
              VersionRangeTable.and(
                VersionRangeTable.cmp(this.atom.version, -1, false, true),
                VersionRangeTable.cmp(this.atom.version, 1, true, false),
              ),
            )
          case '^':
            // `^1.2.3` is equivalent to `>=1.2.3 && <2.0.0 && #flavor`
            return VersionRangeTable.and(
              VersionRangeTable.cmp(this.atom.version, -1, false, true),
              VersionRangeTable.cmp(
                this.atom.version.incrementMajor(),
                -1,
                true,
                false,
              ),
            )
          case '~':
            // `~1.2.3` is equivalent to `>=1.2.3 && <1.3.0 && #flavor`
            return VersionRangeTable.and(
              VersionRangeTable.cmp(this.atom.version, -1, false, true),
              VersionRangeTable.cmp(
                this.atom.version.incrementMinor(),
                -1,
                true,
                false,
              ),
            )
        }
      case 'Flavor':
        return VersionRangeTable.eqFlavor(this.atom.flavor)
      case 'Not':
        return VersionRangeTable.not(this.atom.value.tables())
      case 'And':
        return VersionRangeTable.and(
          this.atom.left.tables(),
          this.atom.right.tables(),
        )
      case 'Or':
        return VersionRangeTable.or(
          this.atom.left.tables(),
          this.atom.right.tables(),
        )
      case 'Any':
        return true
      case 'None':
        return false
    }
  }

  /** Returns `true` if any version exists that could satisfy this range. */
  satisfiable(): boolean {
    return VersionRangeTable.collapse(this.tables()) !== false
  }

  /** Returns `true` if this range and `other` share at least one satisfying version. */
  intersects(other: VersionRange): boolean {
    return VersionRange.and(this, other).satisfiable()
  }

  /**
   * Returns a simplified form with identical version and release satisfaction.
   *
   * Negation and `!=` remain explicit because release evaluation applies them across every
   * declared version. Positive subtrees outside negation use canonical minterm expansion.
   */
  normalize(): VersionRange {
    if (!this.hasReleaseVeto()) {
      return VersionRangeTable.minterms(this.tables())
    }

    switch (this.atom.type) {
      case 'And':
        return VersionRange.and(
          this.atom.left.normalize(),
          this.atom.right.normalize(),
        )
      case 'Or':
        return VersionRange.or(
          this.atom.left.normalize(),
          this.atom.right.normalize(),
        )
      case 'Not':
      default:
        return this
    }
  }

  private hasReleaseVeto(): boolean {
    switch (this.atom.type) {
      case 'Anchor':
        return this.atom.operator === '!='
      case 'Not':
        return true
      case 'And':
      case 'Or':
        return (
          this.atom.left.hasReleaseVeto() || this.atom.right.hasReleaseVeto()
        )
      default:
        return false
    }
  }
}

function numericPrerelease(value: string | number): string | null {
  if (typeof value === 'number') return String(value)
  if (!/^[1-9][0-9]{15,}$/.test(value)) return null
  return value.length > 16 || value > String(Number.MAX_SAFE_INTEGER)
    ? value
    : null
}

/**
 * Represents a semantic version number with numeric segments and optional prerelease identifiers.
 *
 * Follows semver precedence rules: numeric segments are compared left-to-right,
 * and a version with prerelease identifiers has lower precedence than the same version without.
 *
 * @example
 * ```ts
 * const v = Version.parse("1.2.3")
 * console.log(v.toString())             // "1.2.3"
 * console.log(v.compare(Version.parse("1.3.0"))) // "less"
 *
 * const pre = Version.parse("2.0.0-beta.1")
 * console.log(pre.compare(Version.parse("2.0.0"))) // "less" (prerelease < release)
 * ```
 */
export class Version {
  constructor(
    /** The numeric version segments (e.g. `[1, 2, 3]` for `"1.2.3"`). */
    public number: number[],
    /** Numeric identifiers beyond `Number.MAX_SAFE_INTEGER` use decimal strings. */
    public prerelease: (string | number)[],
  ) {}

  /** Serializes this version to its string form (e.g. `"1.2.3"` or `"1.0.0-beta.1"`). */
  toString(): string {
    return `${this.number.join('.')}${this.prerelease.length > 0 ? `-${this.prerelease.join('.')}` : ''}`
  }

  /**
   * Compares this version against another using semver precedence rules.
   *
   * @param other - The version to compare against
   * @returns `'greater'`, `'equal'`, or `'less'`
   */
  compare(other: Version): 'greater' | 'equal' | 'less' {
    const numLen = Math.max(this.number.length, other.number.length)
    for (let i = 0; i < numLen; i++) {
      if ((this.number[i] || 0) > (other.number[i] || 0)) {
        return 'greater'
      } else if ((this.number[i] || 0) < (other.number[i] || 0)) {
        return 'less'
      }
    }

    if (this.prerelease.length === 0 && other.prerelease.length !== 0) {
      return 'greater'
    } else if (this.prerelease.length !== 0 && other.prerelease.length === 0) {
      return 'less'
    }

    const prereleaseLen = Math.max(
      this.prerelease.length,
      other.prerelease.length,
    )
    for (let i = 0; i < prereleaseLen; i++) {
      const left = this.prerelease[i]
      const right = other.prerelease[i]
      if (left === undefined) return 'less'
      if (right === undefined) return 'greater'

      const leftNumeric = numericPrerelease(left)
      const rightNumeric = numericPrerelease(right)
      if (leftNumeric !== null && rightNumeric !== null) {
        if (leftNumeric.length > rightNumeric.length) return 'greater'
        if (leftNumeric.length < rightNumeric.length) return 'less'
        if (leftNumeric > rightNumeric) return 'greater'
        if (leftNumeric < rightNumeric) return 'less'
      } else if (leftNumeric !== null) {
        return 'less'
      } else if (rightNumeric !== null) {
        return 'greater'
      } else {
        if (left > right) return 'greater'
        if (left < right) return 'less'
      }
    }

    return 'equal'
  }

  /**
   * Compares two versions, returning a numeric value suitable for use with `Array.sort()`.
   *
   * @returns `-1` if less, `0` if equal, `1` if greater
   */
  compareForSort(other: Version): -1 | 0 | 1 {
    switch (this.compare(other)) {
      case 'greater':
        return 1
      case 'equal':
        return 0
      case 'less':
        return -1
    }
  }

  /**
   * Parses a version string into a `Version` instance.
   *
   * @param version - A semver-compatible string, e.g. `"1.2.3"` or `"1.0.0-beta.1"`
   * @throws If the string is not a valid version
   */
  static parse(version: string): Version {
    const parsed = P.parse(version, { startRule: 'Version' })
    return new Version(parsed.number, parsed.prerelease)
  }

  /**
   * Returns `true` if this version satisfies the given {@link VersionRange}.
   * Internally treats this as an unflavored {@link ExtendedVersion} with downstream `0`.
   */
  satisfies(versionRange: VersionRange): boolean {
    return new ExtendedVersion(null, this, new Version([0], [])).satisfies(
      versionRange,
    )
  }
}

/**
 * Represents an extended version with an optional flavor, an upstream version, and a downstream version.
 *
 * The format is `#flavor:upstream:downstream` (e.g. `#bitcoin:1.2.3:0`) or `upstream:downstream`
 * for unflavored versions. Flavors allow multiple variants of a package to coexist.
 *
 * - **flavor**: An optional string identifier for the variant (e.g. `"bitcoin"`, `"litecoin"`)
 * - **upstream**: The version of the upstream software being packaged
 * - **downstream**: The version of the StartOS packaging itself
 *
 * Versions with different flavors are incomparable (comparison returns `null`).
 *
 * @example
 * ```ts
 * const v = ExtendedVersion.parse("#bitcoin:1.2.3:0")
 * console.log(v.flavor)      // "bitcoin"
 * console.log(v.upstream)    // Version { number: [1, 2, 3] }
 * console.log(v.downstream)  // Version { number: [0] }
 * console.log(v.toString())  // "#bitcoin:1.2.3:0"
 *
 * const range = VersionRange.parse(">=1.0.0:0")
 * console.log(v.satisfies(range)) // true
 * ```
 */
export class ExtendedVersion {
  constructor(
    /** The flavor identifier (e.g. `"bitcoin"`), or `null` for unflavored versions. */
    public flavor: string | null,
    /** The upstream software version. */
    public upstream: Version,
    /** The downstream packaging version. */
    public downstream: Version,
  ) {}

  /** Serializes this extended version to its string form (e.g. `"#bitcoin:1.2.3:0"` or `"1.0.0:1"`). */
  toString(): string {
    return `${this.flavor ? `#${this.flavor}:` : ''}${this.upstream.toString()}:${this.downstream.toString()}`
  }

  /**
   * Compares this extended version against another.
   *
   * @returns `'greater'`, `'equal'`, `'less'`, or `null` if the flavors differ (incomparable)
   */
  compare(other: ExtendedVersion): 'greater' | 'equal' | 'less' | null {
    if (this.flavor !== other.flavor) {
      return null
    }
    const upstreamCmp = this.upstream.compare(other.upstream)
    if (upstreamCmp !== 'equal') {
      return upstreamCmp
    }
    return this.downstream.compare(other.downstream)
  }

  /**
   * Lexicographic comparison — compares flavors alphabetically first, then versions.
   * Unlike {@link compare}, this never returns `null`: different flavors are ordered alphabetically.
   */
  compareLexicographic(other: ExtendedVersion): 'greater' | 'equal' | 'less' {
    if ((this.flavor || '') > (other.flavor || '')) {
      return 'greater'
    } else if ((this.flavor || '') < (other.flavor || '')) {
      return 'less'
    } else {
      return this.compare(other)!
    }
  }

  /**
   * Returns a numeric comparison result suitable for use with `Array.sort()`.
   * Uses lexicographic ordering (flavors sorted alphabetically, then by version).
   */
  compareForSort(other: ExtendedVersion): 1 | 0 | -1 {
    switch (this.compareLexicographic(other)) {
      case 'greater':
        return 1
      case 'equal':
        return 0
      case 'less':
        return -1
    }
  }

  /** Returns `true` if this version is strictly greater than `other`. Returns `false` if flavors differ. */
  greaterThan(other: ExtendedVersion): boolean {
    return this.compare(other) === 'greater'
  }

  /** Returns `true` if this version is greater than or equal to `other`. Returns `false` if flavors differ. */
  greaterThanOrEqual(other: ExtendedVersion): boolean {
    return ['greater', 'equal'].includes(this.compare(other) as string)
  }

  /** Returns `true` if this version equals `other` (same flavor, upstream, and downstream). */
  equals(other: ExtendedVersion): boolean {
    return this.compare(other) === 'equal'
  }

  /** Returns `true` if this version is strictly less than `other`. Returns `false` if flavors differ. */
  lessThan(other: ExtendedVersion): boolean {
    return this.compare(other) === 'less'
  }

  /** Returns `true` if this version is less than or equal to `other`. Returns `false` if flavors differ. */
  lessThanOrEqual(other: ExtendedVersion): boolean {
    return ['less', 'equal'].includes(this.compare(other) as string)
  }

  /**
   * Parses an extended version string into an `ExtendedVersion`.
   *
   * @param extendedVersion - A string like `"1.2.3:0"` or `"#bitcoin:1.0.0:0"`
   * @throws If the string is not a valid extended version
   */
  static parse(extendedVersion: string): ExtendedVersion {
    const parsed = P.parse(extendedVersion, { startRule: 'ExtendedVersion' })
    return new ExtendedVersion(
      parsed.flavor || null,
      new Version(parsed.upstream.number, parsed.upstream.prerelease),
      new Version(parsed.downstream.number, parsed.downstream.prerelease),
    )
  }

  /**
   * Parses a legacy "emver" format extended version string.
   *
   * @param extendedVersion - A version string in the legacy emver format
   * @throws If the string is not a valid emver version (error message includes the input string)
   */
  static parseEmver(extendedVersion: string): ExtendedVersion {
    try {
      const parsed = P.parse(extendedVersion, { startRule: 'Emver' })
      return new ExtendedVersion(
        parsed.flavor || null,
        new Version(parsed.upstream.number, parsed.upstream.prerelease),
        new Version(parsed.downstream.number, parsed.downstream.prerelease),
      )
    } catch (e) {
      if (e instanceof Error) {
        e.message += ` (${extendedVersion})`
      }
      throw e
    }
  }

  private semanticNumber(): number[] {
    let length = this.upstream.number.length
    while (length > 1 && this.upstream.number[length - 1] === 0) length--
    return this.upstream.number.slice(0, length)
  }

  /** Returns the exclusive upper bound for a caret range anchored at this version. */
  incrementMajor(): ExtendedVersion {
    const semanticNumber = this.semanticNumber()
    const majorIdx = semanticNumber.findIndex(num => num !== 0)
    const incrementIdx = majorIdx === -1 ? semanticNumber.length - 1 : majorIdx
    const incrementedUpstream = new Version(
      semanticNumber
        .slice(0, incrementIdx)
        .concat(semanticNumber[incrementIdx] + 1),
      this.upstream.prerelease,
    )

    return new ExtendedVersion(
      this.flavor,
      incrementedUpstream,
      new Version([0], []),
    )
  }

  /** Returns the exclusive upper bound for a tilde range anchored at this version. */
  incrementMinor(): ExtendedVersion {
    const semanticNumber = this.semanticNumber()
    const majorIdx = semanticNumber.findIndex(num => num !== 0)
    const incrementIdx = majorIdx === -1 ? semanticNumber.length - 1 : majorIdx
    const incrementedUpstream = new Version(
      semanticNumber
        .slice(0, incrementIdx + 1)
        .concat((semanticNumber[incrementIdx + 1] || 0) + 1),
      this.upstream.prerelease,
    )

    return new ExtendedVersion(
      this.flavor,
      incrementedUpstream,
      new Version([0], []),
    )
  }

  /**
   * Returns a boolean indicating whether a given version satisfies the VersionRange
   * !( >= 1:1 <= 2:2) || <=#bitcoin:1.2.0-alpha:0
   */
  satisfies(versionRange: VersionRange): boolean {
    switch (versionRange.atom.type) {
      case 'Anchor':
        const otherVersion = versionRange.atom.version
        switch (versionRange.atom.operator) {
          case '=':
            return this.equals(otherVersion)
          case '>':
            return this.greaterThan(otherVersion)
          case '<':
            return this.lessThan(otherVersion)
          case '>=':
            return this.greaterThanOrEqual(otherVersion)
          case '<=':
            return this.lessThanOrEqual(otherVersion)
          case '!=':
            return !this.equals(otherVersion)
          case '^':
            const nextMajor = versionRange.atom.version.incrementMajor()
            if (
              this.greaterThanOrEqual(otherVersion) &&
              this.lessThan(nextMajor)
            ) {
              return true
            } else {
              return false
            }
          case '~':
            const nextMinor = versionRange.atom.version.incrementMinor()
            if (
              this.greaterThanOrEqual(otherVersion) &&
              this.lessThan(nextMinor)
            ) {
              return true
            } else {
              return false
            }
        }
      case 'Flavor':
        return versionRange.atom.flavor == this.flavor
      case 'And':
        return (
          this.satisfies(versionRange.atom.left) &&
          this.satisfies(versionRange.atom.right)
        )
      case 'Or':
        return (
          this.satisfies(versionRange.atom.left) ||
          this.satisfies(versionRange.atom.right)
        )
      case 'Not':
        return !this.satisfies(versionRange.atom.value)
      case 'Any':
        return true
      case 'None':
        return false
    }
  }
}

/**
 * Compile-time type-checking helper that validates an extended version string literal.
 * If the string is invalid, TypeScript will report a type error at the call site.
 *
 * @example
 * ```ts
 * testTypeExVer("1.2.3:0")         // compiles
 * testTypeExVer("#bitcoin:1.0:0")  // compiles
 * testTypeExVer("invalid")         // type error
 * ```
 */
export const testTypeExVer = <T extends string>(t: T & ValidateExVer<T>) => t

/**
 * Compile-time type-checking helper that validates a version string literal.
 * If the string is invalid, TypeScript will report a type error at the call site.
 *
 * @example
 * ```ts
 * testTypeVersion("1.2.3")  // compiles
 * testTypeVersion("-3")     // type error
 * ```
 */
export const testTypeVersion = <T extends string>(t: T & ValidateVersion<T>) =>
  t

function tests() {
  testTypeVersion('1.2.3')
  testTypeVersion('1')
  testTypeVersion('12.34.56')
  testTypeVersion('1.2-3')
  testTypeVersion('1-3')
  testTypeVersion('1-alpha')
  // @ts-expect-error
  testTypeVersion('-3')
  // @ts-expect-error
  testTypeVersion('1.2.3:1')
  // @ts-expect-error
  testTypeVersion('#cat:1:1')

  testTypeExVer('1.2.3:1.2.3')
  testTypeExVer('1.2.3.4.5.6.7.8.9.0:1')
  testTypeExVer('100:1')
  testTypeExVer('#cat:1:1')
  testTypeExVer('1.2.3.4.5.6.7.8.9.11.22.33:1')
  testTypeExVer('1-0:1')
  testTypeExVer('1-0:1')
  // @ts-expect-error
  testTypeExVer('1.2-3')
  // @ts-expect-error
  testTypeExVer('1-3')
  // @ts-expect-error
  testTypeExVer('1.2.3.4.5.6.7.8.9.0.10:1' as string)
  // @ts-expect-error
  testTypeExVer('1.-2:1')
  // @ts-expect-error
  testTypeExVer('1..2.3:3')
}
