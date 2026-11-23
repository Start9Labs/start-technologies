import { Dump, PatchOp } from './types'

/**
 * Common fields shared by all patch operations.
 */
export interface BaseOperation {
  /** RFC 6901 JSON Pointer targeting the value to operate on. */
  path: string
}

/**
 * An RFC 6902 "add" operation. Inserts {@link value} at {@link path}.
 *
 * @typeParam T - The type of the value being added.
 */
export interface AddOperation<T> extends BaseOperation {
  op: PatchOp.ADD
  value: T
}

/**
 * An RFC 6902 "remove" operation. Deletes the value at {@link path}.
 */
export interface RemoveOperation extends BaseOperation {
  op: PatchOp.REMOVE
}

/**
 * An RFC 6902 "replace" operation. Replaces the value at {@link path} with {@link value}.
 *
 * @typeParam T - The type of the replacement value.
 */
export interface ReplaceOperation<T> extends BaseOperation {
  op: PatchOp.REPLACE
  value: T
}

/**
 * A single RFC 6902 patch operation (add, remove, or replace).
 *
 * @typeParam T - The type of values carried by add/replace operations.
 */
export type Operation<T> =
  AddOperation<T> | RemoveOperation | ReplaceOperation<T>

/**
 * Sentinel value used internally to distinguish a "remove" result from a
 * legitimate `undefined` value in add/replace operations.
 */
// @claude fix #5: Introduced REMOVE_SENTINEL to fix nested array removes.
// Previously, recursiveApply returned `undefined` for remove ops, which was
// indistinguishable from a legitimate undefined value. For nested paths like
// `/arr/0/nested/2`, the array element was set to `undefined` instead of being
// spliced out. Now callers check for REMOVE_SENTINEL to trigger proper splice.
const REMOVE_SENTINEL = Symbol('remove')

/**
 * Retrieves the value at the given JSON Pointer path within a document.
 *
 * @param data - The document to navigate.
 * @param path - An RFC 6901 JSON Pointer string (e.g. `"/users/0/name"`).
 * @returns The value at `path`, or `undefined` if the path doesn't exist.
 *
 * @example
 * ```ts
 * const doc = { users: [{ name: 'Alice' }] }
 * getValueByPointer(doc, '/users/0/name') // 'Alice'
 * getValueByPointer(doc, '/missing')       // undefined
 * ```
 */
export function getValueByPointer<T extends Record<string, T>>(
  data: T,
  path: string,
): any {
  if (!path) return data

  try {
    return arrayFromPath(path).reduce((acc, next) => {
      if (acc == null) return undefined
      return acc[next]
    }, data as any)
  } catch (e) {
    // @claude fix #21: Previously caught all exceptions with `catch (e) { return
    // undefined }`, masking programming errors and state corruption. Now only
    // catches TypeError (from accessing properties on null/undefined), re-throws
    // everything else.
    if (e instanceof TypeError) return undefined
    throw e
  }
}

/**
 * Applies a single RFC 6902 operation to a document, mutating it in place.
 *
 * Objects and arrays along the path are shallow-copied (spread/splice) so that
 * reference identity changes propagate correctly for UI framework change detection.
 *
 * @param doc - The document to modify. The `value` field is replaced with the updated state.
 * @param op - The operation to apply. Must include `path` and `op`; `value` is required for add/replace.
 */
export function applyOperation<T>(
  doc: Dump<Record<string, any>>,
  { path, op, value }: Operation<T> & { value?: T },
) {
  doc.value = recursiveApply(doc.value, arrayFromPath(path), op, value)
}

/**
 * Converts an RFC 6901 JSON Pointer string into an array of unescaped path segments.
 *
 * Handles the RFC 6901 escape sequences: `~1` → `/`, `~0` → `~`.
 *
 * @param path - A JSON Pointer string (e.g. `"/foo/bar~1baz"`).
 * @returns An array of unescaped segments (e.g. `["foo", "bar/baz"]`).
 *
 * @example
 * ```ts
 * arrayFromPath('/users/0/name')   // ['users', '0', 'name']
 * arrayFromPath('/a~1b/c~0d')      // ['a/b', 'c~d']
 * ```
 */
// @claude fix #19: Pre-compiled regex at module scope. Previously, `new RegExp`
// objects were constructed on every call to arrayFromPath/pathFromArray — a hot
// path during patch application. Using regex literals avoids per-call allocation.
const UNESCAPE_TILDE1 = /~1/g
const UNESCAPE_TILDE0 = /~0/g
const ESCAPE_TILDE = /~/g
const ESCAPE_SLASH = /\//g

export function arrayFromPath(path: string): string[] {
  return path
    .split('/')
    .slice(1)
    .map(p =>
      // order matters, always replace "~1" first
      p.replace(UNESCAPE_TILDE1, '/').replace(UNESCAPE_TILDE0, '~'),
    )
}

/**
 * Converts an array of path segments into an RFC 6901 JSON Pointer string.
 *
 * Handles the RFC 6901 escape sequences: `~` → `~0`, `/` → `~1`.
 *
 * @param args - Path segments (strings or numbers).
 * @returns A JSON Pointer string, or `""` (root) if `args` is empty.
 *
 * @example
 * ```ts
 * pathFromArray(['users', 0, 'name'])  // '/users/0/name'
 * pathFromArray([])                     // ''
 * ```
 */
export function pathFromArray(args: Array<string | number>): string {
  if (!args.length) return ''

  return (
    '/' +
    args
      .map(a =>
        String(a)
          // do not change order, "~" needs to be replaced first
          .replace(ESCAPE_TILDE, '~0')
          .replace(ESCAPE_SLASH, '~1'),
      )
      .join('/')
  )
}

/**
 * Resolves an RFC 6902 array index from a path segment string.
 * Handles the special "-" token (end-of-array for add operations).
 */
// @claude fix #6: Previously, `parseInt("-")` returned NaN, and
// `splice(NaN, 0, value)` silently inserted at position 0 instead of
// appending. Non-numeric segments also produced corrupt state without error.
// Now explicitly handles "-" per RFC 6902 and validates numeric indices.
function resolveArrayIndex(segment: string, arrayLength: number): number {
  if (segment === '-') return arrayLength
  const index = Number(segment)
  if (!Number.isInteger(index) || index < 0) {
    throw new Error(`Invalid array index "${segment}" in JSON Patch path`)
  }
  return index
}

function recursiveApply<T extends Record<string, any> | any[]>(
  data: T,
  path: readonly string[],
  op: PatchOp,
  value?: any,
): T {
  // Base case: path fully consumed
  if (!path.length) {
    // For remove operations, return a sentinel so callers can distinguish
    // "remove this key" from "set this key to undefined".
    if (op === PatchOp.REMOVE) return REMOVE_SENTINEL as any
    return value
  }

  // object
  if (isObject(data)) {
    return recursiveApplyObject(data, path, op, value)
    // array
  } else if (Array.isArray(data)) {
    return recursiveApplyArray(data, path, op, value)
  } else {
    // @claude fix #22: Previously `throw 'unreachable'` — a string with no
    // stack trace. Now throws a proper Error with a descriptive message.
    throw new Error(
      `Cannot apply patch at path segment "${path[0]}": ` +
        `expected object or array but found ${typeof data}`,
    )
  }
}

function recursiveApplyObject<T extends Record<string, any>>(
  data: T,
  path: readonly string[],
  op: PatchOp,
  value?: any,
): T {
  const segment = path[0] || ''
  const updated = recursiveApply(data[segment], path.slice(1), op, value)
  const result = {
    ...data,
    [segment]: updated,
  }

  if (updated === REMOVE_SENTINEL) {
    delete result[segment]
  }

  return result
}

function recursiveApplyArray<T extends any[]>(
  data: T,
  path: readonly string[],
  op: PatchOp,
  value?: any,
): T {
  const segment = path[0] || ''
  const result = [...data] as T

  if (path.length === 1 && op === PatchOp.ADD) {
    // RFC 6902: add with "-" appends to the end
    const index = resolveArrayIndex(segment, data.length)
    result.splice(index, 0, value)
  } else if (path.length === 1 && op === PatchOp.REMOVE) {
    const index = resolveArrayIndex(segment, data.length)
    result.splice(index, 1)
  } else {
    const index = resolveArrayIndex(segment, data.length)
    const updated = recursiveApply(data[index], path.slice(1), op, value)
    if (updated === REMOVE_SENTINEL) {
      // Nested remove targeting an array element — splice it out
      result.splice(index, 1)
    } else {
      result[index] = updated
    }
  }

  return result
}

/**
 * Scopes a patch to the subtree at `pointer`, as the server scopes a subscription's
 * revisions: operations below it become relative, and an operation on an ancestor
 * becomes one on the subtree root.
 */
export function scopeOperations(
  patch: Operation<unknown>[],
  pointer: string,
): Operation<unknown>[] {
  return patch.flatMap((op): Operation<unknown>[] => {
    if (
      op.path === pointer ||
      op.path === '' ||
      pointer.startsWith(op.path + '/')
    ) {
      if (op.op === PatchOp.REMOVE)
        return [{ op: PatchOp.REPLACE, path: '', value: null }]
      return [
        {
          ...op,
          path: '',
          value:
            getValueByPointer(op.value as any, pointer.slice(op.path.length)) ??
            null,
        },
      ]
    }
    if (op.path.startsWith(pointer + '/'))
      return [{ ...op, path: op.path.slice(pointer.length) }]
    return []
  })
}

function isObject(val: any): val is Record<string, unknown> {
  return typeof val === 'object' && val !== null && !Array.isArray(val)
}
