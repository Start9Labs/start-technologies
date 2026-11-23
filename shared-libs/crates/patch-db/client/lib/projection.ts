import { arrayFromPath, Operation } from './json-patch-lib'
import { PatchOp } from './types'

/**
 * The part of a subscribed subtree a subscriber receives, mirroring the server's `Projection`.
 *
 * Patterns are pointers relative to the subtree, and a `*` segment matches any single key or
 * index. Array elements must be addressed by `*`. An empty `include` keeps everything.
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

/** Null for a projection that keeps everything; patterns sorted and deduplicated otherwise. */
export function normalizeProjection(
  projection: Projection | null | undefined,
): NormalizedProjection | null {
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
