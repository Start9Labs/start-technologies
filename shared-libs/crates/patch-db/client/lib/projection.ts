import { arrayFromPath, Operation, pathFromArray } from './json-patch-lib'
import { PatchOp } from './types'

/**
 * One included subtree, mirroring the server's `Selector`. Patterns are pointers relative to
 * the subscribed subtree, where a `*` segment matches any single key or index; `depth`
 * counts levels below the subscribed subtree. Array-position segments (digits or `-`) are
 * rejected.
 */
export type Selector = {
  include: string
  exclude?: readonly string[] | null
  depth?: number | null
}

/**
 * What one watch keeps of the subtree at its path: patterns relative to that path, and
 * an empty `include` keeps everything.
 */
export type Projection = {
  depth?: number | null
  include?: readonly string[] | null
  exclude?: readonly string[] | null
}

/** A union of selectors, mirroring the server's `Projection`; empty keeps everything. */
export type Selection = readonly Selector[]

/** Keeps nothing, which an empty selection cannot express. */
export const NOTHING: Selection = [{ include: '', exclude: [''] }]

export type Reach = 'out' | 'route' | 'in'

const RANK: Record<Reach, number> = { out: 0, route: 1, in: 2 }

/** Whether a pattern segment would select an array position. */
export function isPositional(segment: string): boolean {
  return segment === '-' || /^[0-9]+$/.test(segment)
}

/** The longest prefix of a path without an array-position segment. */
export function beforePositional(path: readonly string[]): string[] {
  const at = path.findIndex(isPositional)
  return at < 0 ? [...path] : path.slice(0, at)
}

function assertNotPositional(pattern: string) {
  if (arrayFromPath(pattern).some(isPositional))
    throw new Error(
      `Projection pattern ${pattern} addresses an array position; use *`,
    )
}

/** A selector as the server's parameters take it. */
export type WireSelector = { include: string; exclude?: string[]; depth?: number }

/** Patterns sorted and deduplicated; throws for an array-position segment. */
export function normalizeSelector(selector: Selector): WireSelector {
  assertNotPositional(selector.include)
  const exclude = [...new Set(selector.exclude ?? [])].sort()
  exclude.forEach(assertNotPositional)
  const depth = selector.depth ?? null
  return {
    include: selector.include,
    ...(exclude.length ? { exclude } : {}),
    ...(depth === null ? {} : { depth }),
  }
}

/** Identifies a selector; its include path alone when it excludes nothing and has no depth. */
export function selectorKey(selector: Selector): string {
  const { include, exclude, depth } = normalizeSelector(selector)
  return exclude || depth !== undefined
    ? JSON.stringify([include, { exclude, depth }])
    : include
}

export function parseSelectorKey(key: string): Selector {
  if (!key.startsWith('[')) return { include: key }
  const [include, rest] = JSON.parse(key) as [string, Omit<Selector, 'include'>]
  return normalizeSelector({ include, ...rest })
}

/** Identifies a watch; its pointer alone when it keeps everything there. */
export function subscriptionKey(
  pointer: string,
  projection: Projection | null | undefined,
): string {
  const normalized = normalizeProjection(projection)
  return normalized ? JSON.stringify([pointer, normalized]) : pointer
}

export function parseSubscriptionKey(key: string): {
  pointer: string
  projection: Projection | null
} {
  if (!key.startsWith('[')) return { pointer: key, projection: null }
  const [pointer, projection] = JSON.parse(key) as [string, Projection]
  return { pointer, projection }
}

function normalizeProjection(
  projection: Projection | null | undefined,
): Projection | null {
  const depth = projection?.depth ?? null
  const include = [...new Set(projection?.include ?? [])].sort()
  const exclude = [...new Set(projection?.exclude ?? [])].sort()
  if (depth === null && !include.length && !exclude.length) return null
  return {
    ...(depth === null ? {} : { depth }),
    ...(include.length ? { include } : {}),
    ...(exclude.length ? { exclude } : {}),
  }
}

/** A watch's projection as selectors relative to its own path. */
export function watchSelection(projection: Projection | null): Selection {
  if (!projection) return []
  const include = projection.include?.length ? projection.include : ['']
  return include.map(include => ({
    include,
    exclude: projection.exclude ?? undefined,
    depth: projection.depth ?? undefined,
  }))
}

/**
 * The selectors a watch needs, relative to the subscribed root. Array-position segments
 * widen a pattern to the part before them, and drop an exclusion.
 */
export function watchSelectors(key: string): Selector[] {
  const { pointer, projection } = parseSubscriptionKey(key)
  const base = arrayFromPath(pointer)
  const at = (pattern: string) => [...base, ...arrayFromPath(pattern)]
  const selection = watchSelection(projection)
  return (selection.length ? selection : [{ include: '' }]).map(selector => {
    const include = at(selector.include)
    const kept = beforePositional(include)
    const widened = kept.length < include.length
    return normalizeSelector({
      include: pathFromArray(kept),
      exclude: widened
        ? []
        : (selector.exclude ?? [])
            .map(at)
            .filter(exclude => !exclude.some(isPositional))
            .map(pathFromArray),
      depth:
        widened || selector.depth == null ? null : base.length + selector.depth,
    })
  })
}

function segmentMatches(pattern: string, segment: string): boolean {
  return pattern === '*' || pattern === segment
}

function covers(pattern: string[], path: readonly string[]): boolean {
  return (
    pattern.length <= path.length &&
    pattern.every((p, i) => segmentMatches(p, path[i]!))
  )
}

function leadsThrough(pattern: string[], path: readonly string[]): boolean {
  return (
    pattern.length > path.length &&
    path.every((s, i) => segmentMatches(pattern[i]!, s))
  )
}

type Parsed = { include: string[]; exclude: string[][]; depth: number | null }

const parsed = new WeakMap<Selector, Parsed>()

function parse(selector: Selector): Parsed {
  let found = parsed.get(selector)
  if (!found) {
    found = {
      include: arrayFromPath(selector.include),
      exclude: (selector.exclude ?? []).map(arrayFromPath),
      depth: selector.depth ?? null,
    }
    parsed.set(selector, found)
  }
  return found
}

function selectorReach(selector: Selector, path: readonly string[]): Reach {
  const { include, exclude, depth } = parse(selector)
  if (depth !== null && path.length > depth) return 'out'
  if (exclude.some(p => covers(p, path))) return 'out'
  if (covers(include, path)) return 'in'
  if (leadsThrough(include, path)) return 'route'
  return 'out'
}

function keepsWhole(selector: Selector, path: readonly string[]): boolean {
  const { include, exclude, depth } = parse(selector)
  return (
    depth === null &&
    covers(include, path) &&
    !exclude.some(p => covers(p, path) || leadsThrough(p, path))
  )
}

export function reach(selection: Selection, path: readonly string[]): Reach {
  if (!selection.length) return 'in'
  return selection
    .map(selector => selectorReach(selector, path))
    .reduce((a, b) => (RANK[b] > RANK[a] ? b : a), 'out' as Reach)
}

function wholeBelow(selection: Selection, path: readonly string[]): boolean {
  return !selection.length || selection.some(s => keepsWhole(s, path))
}

/** The projected value at a path the selection reaches. */
export function projectValue(
  selection: Selection,
  path: readonly string[],
  value: unknown,
): unknown {
  if (wholeBelow(selection, path)) return value
  if (Array.isArray(value))
    return value.flatMap((child, i) => {
      const at = [...path, String(i)]
      return reach(selection, at) === 'out'
        ? []
        : [projectValue(selection, at, child)]
    })
  if (isObject(value))
    return Object.fromEntries(
      Object.entries(value).flatMap(([key, child]) => {
        const at = [...path, key]
        return reach(selection, at) === 'out'
          ? []
          : [[key, projectValue(selection, at, child)]]
      }),
    )
  return value
}

/** The projected subtree value; null when the selection excludes the subtree itself. */
export function projectRoot(selection: Selection, value: unknown): unknown {
  return reach(selection, []) === 'out'
    ? null
    : projectValue(selection, [], value)
}

/** Restricts operations already scoped to the subtree, as the server's `Projection` does. */
export function projectOperations(
  patch: Operation<unknown>[],
  selection: Selection,
): Operation<unknown>[] {
  if (!selection.length) return patch
  return patch.flatMap((op): Operation<unknown>[] => {
    const path = arrayFromPath(op.path)
    if (reach(selection, path) === 'out') return []
    if (op.op === PatchOp.REMOVE) return [op]
    return [{ ...op, value: projectValue(selection, path, op.value) }]
  })
}

function gains(held: Selection, wider: Selection, path: string[]): boolean {
  const before = reach(held, path)
  const after = reach(wider, path)
  return (
    (before === 'out' && after !== 'out') || (after === 'in' && before !== 'in')
  )
}

/**
 * Operations that take a replica holding one selection's projection, re-projected to the
 * wider selection, to the wider selection's projection; mirrors `Projection::extension`.
 */
export function extension(
  held: Selection,
  wider: Selection,
  value: unknown,
): Operation<unknown>[] {
  const ops: Operation<unknown>[] = []
  const set = (path: string[], inArray: boolean, value: unknown) =>
    ops.push({
      op: inArray || !path.length ? PatchOp.REPLACE : PatchOp.ADD,
      path: pathFromArray(path),
      value: projectValue(wider, path, value),
    } as Operation<unknown>)
  const walk = (path: string[], inArray: boolean, value: unknown) => {
    if (gains(held, wider, path)) return set(path, inArray, value)
    if (wholeBelow(held, path)) return
    if (Array.isArray(value)) {
      const shifted = value.some(
        (_, i) =>
          reach(held, [...path, String(i)]) === 'out' &&
          reach(wider, [...path, String(i)]) !== 'out',
      )
      if (shifted) return set(path, inArray, value)
      value.forEach((child, i) => {
        if (reach(wider, [...path, String(i)]) !== 'out')
          walk([...path, String(i)], true, child)
      })
    } else if (isObject(value)) {
      Object.entries(value).forEach(([key, child]) => {
        if (reach(wider, [...path, key]) !== 'out')
          walk([...path, key], false, child)
      })
    }
  }
  if (reach(wider, []) !== 'out') walk([], false, value)
  return ops
}

function subsumes(general: string[], specific: string[]): boolean {
  return (
    general.length <= specific.length &&
    general.every((g, i) => g === '*' || g === specific[i])
  )
}

function compatible(a: string[], b: string[]): boolean {
  return a.every(
    (s, i) => i >= b.length || s === '*' || b[i] === '*' || s === b[i],
  )
}

/** Whether one selector keeps everything another does. Conservative: unprovable is false. */
export function selectorContains(server: Selector, served: Selector): boolean {
  const outer = parse(server)
  const inner = parse(served)
  if (!subsumes(outer.include, inner.include)) return false
  if (
    outer.depth !== null &&
    (inner.depth === null || inner.depth > outer.depth)
  )
    return false
  return outer.exclude.every(
    e =>
      !compatible(e, inner.include) ||
      (inner.depth !== null && e.length > inner.depth) ||
      inner.exclude.some(x => subsumes(x, e)),
  )
}

/** Whether a selection keeps everything a selector does. */
export function selectionContains(
  selection: Selection,
  selector: Selector,
): boolean {
  return (
    !selection.length || selection.some(s => selectorContains(s, selector))
  )
}

/** The selectors no other one contains, keyed for comparison. */
export function minimalSelection(selectors: readonly Selector[]): Selection {
  const unique = [
    ...new Map(selectors.map(s => [selectorKey(s), normalizeSelector(s)])),
  ].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
  return unique
    .filter(
      ([key, selector]) =>
        !unique.some(
          ([other, s]) =>
            other !== key &&
            selectorContains(s, selector) &&
            (!selectorContains(selector, s) || other < key),
        ),
    )
    .map(([, selector]) => selector)
}

export function selectionKey(selection: Selection): string {
  return JSON.stringify(selection.map(selectorKey).sort())
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
