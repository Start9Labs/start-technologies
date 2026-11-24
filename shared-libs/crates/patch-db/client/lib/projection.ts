import { arrayFromPath, Operation, pathFromArray } from './json-patch-lib'
import { PatchOp } from './types'

/**
 * The part of a subscribed subtree a subscriber receives, mirroring the server's `Projection`.
 *
 * Patterns are pointers relative to the subtree, and a `*` segment matches any single key or
 * index. An empty `include` keeps everything. Segments that are array positions (digits or
 * `-`) are rejected, so array elements are addressed only by `*`.
 */
export type Projection = {
  /** Levels kept below the subtree; deeper objects and arrays become empty. */
  depth?: number | null
  include?: readonly string[] | null
  /** Subtrees removed from what `include` keeps. */
  exclude?: readonly string[] | null
}

export type Reach = 'out' | 'route' | 'in'

export type NormalizedProjection = {
  depth?: number
  include?: string[]
  exclude?: string[]
}

/** Whether a pattern segment would select an array position. */
export function isPositional(segment: string): boolean {
  return segment === '-' || /^[0-9]+$/.test(segment)
}

/**
 * Null for a projection that keeps everything; patterns sorted and deduplicated otherwise.
 * Throws for a positional pattern segment, as the server rejects it.
 */
export function normalizeProjection(
  projection: Projection | null | undefined,
): NormalizedProjection | null {
  const depth = projection?.depth ?? null
  const include = [...new Set(projection?.include ?? [])].sort()
  const exclude = [...new Set(projection?.exclude ?? [])].sort()
  const positional = [...include, ...exclude].find(p =>
    arrayFromPath(p).some(isPositional),
  )
  if (positional !== undefined)
    throw new Error(
      `Projection pattern ${positional} addresses an array position; use *`,
    )
  if (depth === null && !include.length && !exclude.length) return null
  return {
    ...(depth === null ? {} : { depth }),
    ...(include.length ? { include } : {}),
    ...(exclude.length ? { exclude } : {}),
  }
}

/** Identifies a subscription; a pointer alone when nothing is projected. */
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

const parsed = new WeakMap<
  Projection,
  { include: string[][]; exclude: string[][] }
>()

function patterns(projection: Projection) {
  let found = parsed.get(projection)
  if (!found) {
    found = {
      include: (projection.include ?? []).map(arrayFromPath),
      exclude: (projection.exclude ?? []).map(arrayFromPath),
    }
    parsed.set(projection, found)
  }
  return found
}

export function reach(
  projection: Projection | null,
  path: readonly string[],
): Reach {
  if (!projection) return 'in'
  const depth = projection.depth ?? null
  if (depth !== null && path.length > depth) return 'out'
  const { include, exclude } = patterns(projection)
  if (exclude.some(p => covers(p, path))) return 'out'
  if (!include.length || include.some(p => covers(p, path))) return 'in'
  if (include.some(p => leadsThrough(p, path))) return 'route'
  return 'out'
}

function wholeBelow(projection: Projection, path: readonly string[]): boolean {
  return (
    (projection.depth ?? null) === null &&
    !patterns(projection).exclude.some(p => leadsThrough(p, path))
  )
}

/** The projected value at a path the projection reaches. */
export function projectValue(
  projection: Projection | null,
  path: readonly string[],
  value: unknown,
): unknown {
  if (!projection) return value
  if (reach(projection, path) === 'in' && wholeBelow(projection, path))
    return value
  if (Array.isArray(value))
    return value.flatMap((child, i) => {
      const at = [...path, String(i)]
      return reach(projection, at) === 'out'
        ? []
        : [projectValue(projection, at, child)]
    })
  if (isObject(value))
    return Object.fromEntries(
      Object.entries(value).flatMap(([key, child]) => {
        const at = [...path, key]
        return reach(projection, at) === 'out'
          ? []
          : [[key, projectValue(projection, at, child)]]
      }),
    )
  return value
}

/** The projected subtree value; null when the projection excludes the subtree itself. */
export function projectRoot(
  projection: Projection | null,
  value: unknown,
): unknown {
  if (!projection) return value
  return reach(projection, []) === 'out'
    ? null
    : projectValue(projection, [], value)
}

/** Restricts operations already scoped to the subtree, as the server's `Projection` does. */
export function projectOperations(
  patch: Operation<unknown>[],
  projection: Projection | null,
): Operation<unknown>[] {
  if (!projection) return patch
  return patch.flatMap((op): Operation<unknown>[] => {
    const path = arrayFromPath(op.path)
    if (reach(projection, path) === 'out') return []
    if (op.op === PatchOp.REMOVE) return [op]
    return [{ ...op, value: projectValue(projection, path, op.value) }]
  })
}

/**
 * Combines a projected value with the document's current value at the same path.
 * Paths outside the projection keep their current value.
 */
export function mergeProjected(
  projection: Projection | null,
  path: readonly string[],
  current: unknown,
  projected: unknown,
): unknown {
  if (!projection) return projected
  const here = reach(projection, path)
  if (here === 'out') return current
  if (here === 'in' && wholeBelow(projection, path)) return projected
  if (Array.isArray(projected)) {
    const elementsOut = reach(projection, [...path, '*']) === 'out'
    if (elementsOut) return Array.isArray(current) ? current : projected
    return projected.map((child, i) =>
      mergeProjected(
        projection,
        [...path, String(i)],
        Array.isArray(current) ? current[i] : undefined,
        child,
      ),
    )
  }
  if (isObject(projected)) {
    const base = isObject(current) ? current : {}
    const merged: Record<string, unknown> = {}
    Object.entries(base).forEach(([key, child]) => {
      if (reach(projection, [...path, key]) === 'out') merged[key] = child
    })
    Object.entries(projected).forEach(([key, child]) => {
      merged[key] = mergeProjected(projection, [...path, key], base[key], child)
    })
    return merged
  }
  return projected
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

type Scoped = {
  path: string[]
  depth: number | null
  include: string[][]
  exclude: string[][]
}

/** A subscription key's projection, as patterns relative to an ancestor pointer. */
function scopedTo(base: string[], key: string): Scoped {
  const { pointer, projection } = parseSubscriptionKey(key)
  const path = arrayFromPath(pointer)
  const offset = path.slice(base.length)
  const include = (projection?.include ?? []).map(p => [
    ...offset,
    ...arrayFromPath(p),
  ])
  return {
    path,
    depth:
      projection?.depth === null || projection?.depth === undefined
        ? null
        : offset.length + projection.depth,
    include: include.length ? include : [offset],
    exclude: (projection?.exclude ?? []).map(p => [
      ...offset,
      ...arrayFromPath(p),
    ]),
  }
}

/** Whether every path the specific pattern matches, the general one matches too. */
function subsumes(general: string[], specific: string[]): boolean {
  return (
    general.length <= specific.length &&
    general.every((g, i) => g === '*' || g === specific[i])
  )
}

/** Whether two patterns can match a common path. */
function compatible(a: string[], b: string[]): boolean {
  return a.every(
    (s, i) => i >= b.length || s === '*' || b[i] === '*' || s === b[i],
  )
}

function startsWithPath(path: string[], prefix: string[]): boolean {
  return prefix.length <= path.length && prefix.every((s, i) => s === path[i])
}

/**
 * Whether one subscription's data includes everything another's does. Conservative:
 * an unprovable containment is false.
 */
export function contains(server: string, served: string): boolean {
  if (server === served) return true
  const { pointer, projection } = parseSubscriptionKey(server)
  const base = arrayFromPath(pointer)
  const inner = scopedTo(base, served)
  if (!startsWithPath(inner.path, base)) return false
  if (!projection) return true
  const outer = scopedTo(base, server)
  const offset = inner.path.slice(base.length)
  if (outer.depth !== null && (inner.depth === null || inner.depth > outer.depth))
    return false
  if (!inner.include.every(i => outer.include.some(o => subsumes(o, i))))
    return false
  return outer.exclude.every(e => {
    if (!compatible(e, offset)) return true
    if (inner.depth !== null && e.length > inner.depth) return true
    if (e.length <= offset.length)
      return inner.exclude.some(x => x.length === offset.length)
    const below = e.slice(offset.length)
    return inner.exclude.some(x => subsumes(x.slice(offset.length), below))
  })
}

/** Whether two subscriptions' subtrees share any path. */
export function overlaps(a: string, b: string): boolean {
  const p = arrayFromPath(parseSubscriptionKey(a).pointer)
  const q = arrayFromPath(parseSubscriptionKey(b).pointer)
  return startsWithPath(p, q) || startsWithPath(q, p)
}

/**
 * One subscription containing every given subscription, at their common ancestor. A
 * member pointer with an array-position segment widens it to the whole subtree.
 */
export function covering(keys: readonly string[]): string {
  const pointers = keys.map(key => arrayFromPath(parseSubscriptionKey(key).pointer))
  const base = pointers.reduce((a, b) => {
    const differs = a.findIndex((s, i) => s !== b[i])
    return differs < 0 ? a.slice(0, b.length) : a.slice(0, differs)
  })
  const scoped = keys.map(key => scopedTo(base, key))
  const include = scoped.some(s => s.include.some(p => !p.length))
    ? []
    : scoped.flatMap(s => s.include)
  const [first, ...rest] = scoped
  const exclude = first!.exclude.filter(e =>
    rest.every(s =>
      s.exclude.some(x => pathFromArray(x) === pathFromArray(e)),
    ),
  )
  const depth = scoped.every(s => s.depth !== null)
    ? Math.max(...scoped.map(s => s.depth!))
    : null
  if ([...include, ...exclude].some(p => p.some(isPositional)))
    return pathFromArray(base)
  return subscriptionKey(pathFromArray(base), {
    depth,
    include: include.map(pathFromArray),
    exclude: exclude.map(pathFromArray),
  })
}
