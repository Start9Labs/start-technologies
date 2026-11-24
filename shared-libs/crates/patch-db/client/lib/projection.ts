import {
  arrayFromPath,
  getValueByPointer,
  Operation,
  pathFromArray,
  scopeOperations,
} from './json-patch-lib'
import { PatchOp } from './types'

/**
 * One included subtree, mirroring the server's `Selector`. Patterns are pointers relative to
 * the subscribed subtree, where a `*` segment matches any single key or index and a numeric
 * segment that key or array position; `depth` counts levels below the subscribed subtree.
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

/** A selector as the server's parameters take it. */
export type WireSelector = { include: string; exclude?: string[]; depth?: number }

/** Patterns sorted and deduplicated. */
export function normalizeSelector(selector: Selector): WireSelector {
  const exclude = [...new Set(selector.exclude ?? [])].sort()
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

/** The selectors a watch needs, relative to the subscribed root. */
export function watchSelectors(key: string): Selector[] {
  const { pointer, projection } = parseSubscriptionKey(key)
  const base = arrayFromPath(pointer)
  const at = (pattern: string) =>
    pathFromArray([...base, ...arrayFromPath(pattern)])
  const selection = watchSelection(projection)
  return (selection.length ? selection : [{ include: '' }]).map(selector =>
    normalizeSelector({
      include: at(selector.include),
      exclude: (selector.exclude ?? []).map(at),
      depth: selector.depth == null ? null : base.length + selector.depth,
    }),
  )
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

/** Whether depth drops every child of the path, which then projects as an empty container. */
function cutsBelow(selection: Selection, path: readonly string[]): boolean {
  return (
    !!selection.length &&
    selection
      .filter(s => selectorReach(s, path) !== 'out')
      .every(s => {
        const { depth } = parse(s)
        return depth !== null && depth <= path.length
      })
  )
}

/** The projected value at a path the selection reaches. */
export function projectValue(
  selection: Selection,
  path: readonly string[],
  value: unknown,
): unknown {
  if (wholeBelow(selection, path)) return value
  if (Array.isArray(value)) {
    if (cutsBelow(selection, path)) return []
    return value.map((child, i) => {
      const at = [...path, String(i)]
      return reach(selection, at) === 'out'
        ? null
        : projectValue(selection, at, child)
    })
  }
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

function projectOperation(
  selection: Selection,
  op: Operation<unknown>,
): Operation<unknown>[] {
  const path = arrayFromPath(op.path)
  if (reach(selection, path) === 'out') return []
  if (op.op === PatchOp.REMOVE) return [op]
  return [{ ...op, value: projectValue(selection, path, op.value) }]
}

/**
 * Restricts operations already scoped to the subtree. An insertion into or removal from an
 * array the replica holds stays, with a null placeholder for an element the selection drops,
 * so positions stay aligned.
 */
export function projectOperations(
  patch: Operation<unknown>[],
  selection: Selection,
  isArray: (path: string[]) => boolean = () => false,
): Operation<unknown>[] {
  if (!selection.length) return patch
  return patch.flatMap((op): Operation<unknown>[] => {
    const path = arrayFromPath(op.path)
    if (
      op.op !== PatchOp.REPLACE &&
      path.length &&
      isArray(path.slice(0, -1)) &&
      reach(selection, path.slice(0, -1)) !== 'out'
    ) {
      if (op.op === PatchOp.REMOVE) return [op]
      return [
        {
          ...op,
          value:
            reach(selection, path) === 'out'
              ? null
              : projectValue(selection, path, op.value),
        },
      ]
    }
    return projectOperation(selection, op)
  })
}

/** The arrays a patch inserts into or removes from, read from the document after it. */
export function shiftedArrays(
  patch: Operation<unknown>[],
  after: unknown,
): string[] {
  const arrays = new Set<string>()
  patch.forEach(op => {
    if (op.op === PatchOp.REPLACE) return
    const path = arrayFromPath(op.path)
    if (!path.length) return
    const parent = pathFromArray(path.slice(0, -1))
    if (Array.isArray(getValueByPointer(after as any, parent))) arrays.add(parent)
  })
  return [...arrays]
}

/**
 * A patch scoped to the subtree at `pointer` and restricted to the selection, as the server's
 * `Projection::project_scoped` sends it: arrays the patch shifts are sent whole from the
 * document after it unless the selection keeps them whole, and so is the subtree itself when
 * it lies inside one.
 */
export function projectScoped(
  patch: Operation<unknown>[],
  pointer: string,
  selection: Selection,
  after: unknown,
): Operation<unknown>[] {
  const shifted = shiftedArrays(patch, after)
  const root = arrayFromPath(pointer)
  const valueAt = (path: string[]) =>
    getValueByPointer(after as any, pathFromArray([...root, ...path]))
  if (
    shifted.some(array => {
      const at = arrayFromPath(array)
      return at.length < root.length && at.every((s, i) => s === root[i])
    })
  )
    return [
      {
        op: PatchOp.REPLACE,
        path: '',
        value: projectRoot(selection, valueAt([]) ?? null),
      },
    ]
  const resent = shifted
    .map(arrayFromPath)
    .filter(
      at => at.length >= root.length && root.every((s, i) => s === at[i]),
    )
    .map(at => at.slice(root.length))
    .filter(
      rel => reach(selection, rel) !== 'out' && !wholeBelow(selection, rel),
    )
    .sort((a, b) => a.length - b.length)
    .filter(
      (rel, i, all) =>
        !all
          .slice(0, i)
          .some(outer => outer.every((s, k) => s === rel[k])),
    )
  const ops = scopeOperations(patch, pointer)
  const under = (op: Operation<unknown>, rel: string[]) => {
    const path = arrayFromPath(op.path)
    return rel.every((s, k) => s === path[k])
  }
  const last = resent.map(rel => {
    let found = -1
    ops.forEach((op, i) => {
      if (under(op, rel)) found = i
    })
    return found
  })
  return ops.flatMap((op, i): Operation<unknown>[] => {
    const array = resent.findIndex(rel => under(op, rel))
    if (array < 0) return projectOperation(selection, op)
    if (last[array] !== i) return []
    const rel = resent[array]!
    const value = valueAt(rel)
    return value === undefined
      ? []
      : [
          {
            op: PatchOp.REPLACE,
            path: pathFromArray(rel),
            value: projectValue(selection, rel, value),
          } as Operation<unknown>,
        ]
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
      if (cutsBelow(held, path) && !cutsBelow(wider, path))
        return set(path, inArray, value)
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
