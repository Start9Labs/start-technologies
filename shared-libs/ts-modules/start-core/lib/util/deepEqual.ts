/**
 * Performs a deep structural equality check across all provided arguments.
 * Returns true only if every argument is deeply equal to every other argument.
 * Handles primitives, arrays, and plain objects (JSON-like) recursively.
 *
 * Non-plain objects (Set, Map, Date, etc.) are compared by reference only,
 * since Object.keys() does not enumerate their contents.
 *
 * @param args - Two or more values to compare for deep equality
 * @returns True if all arguments are deeply equal
 *
 * @example
 * ```ts
 * deepEqual({ a: 1 }, { a: 1 })           // true
 * deepEqual([1, 2], [1, 2], [1, 2])       // true
 * deepEqual({ a: 1 }, { a: 2 })           // false
 * ```
 */
export function deepEqual(...args: unknown[]) {
  if (args.every(x => x === args[0])) return true
  const objects = args.filter(
    (x): x is object => typeof x === 'object' && x !== null,
  )
  if (objects.length === 0) {
    for (const x of args) if (x !== args[0]) return false
    return true
  }
  if (objects.length !== args.length) return false
  if (objects.some(Array.isArray) && !objects.every(Array.isArray)) return false
  if (
    objects.some(
      x => !Array.isArray(x) && Object.getPrototypeOf(x) !== Object.prototype,
    )
  ) {
    return (
      objects.reduce<object | null>(
        (a, b) => (a === b ? a : null),
        objects[0],
      ) !== null
    )
  }
  const allKeys = new Set(objects.flatMap(x => Object.keys(x)))
  for (const key of allKeys) {
    for (const x of objects) {
      if (!(key in x)) return false
      if (!deepEqual((objects[0] as any)[key], (x as any)[key])) return false
    }
  }
  return true
}
