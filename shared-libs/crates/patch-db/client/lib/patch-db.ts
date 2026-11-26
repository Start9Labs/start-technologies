import { Dump, Revision, Update } from './types'
import {
  BehaviorSubject,
  filter,
  Observable,
  Subscription,
  switchMap,
  take,
} from 'rxjs'
import {
  applyOperation,
  arrayFromPath,
  getValueByPointer,
  pathFromArray,
} from './json-patch-lib'
import { reconcile } from './projection'

/**
 * Observable database client backed by RFC 6902 JSON Patches.
 *
 * Consumes a stream of {@link Update}s (either full {@link Dump}s or incremental
 * {@link Revision}s) from a server, maintains a local cache, and exposes reactive
 * `watch$()` observables for any subtree of the document.
 *
 * @typeParam T - The shape of the root document.
 *
 * @example
 * ```ts
 * interface AppState {
 *   users: { [id: string]: { name: string } }
 *   settings: { theme: string }
 * }
 *
 * const db = new PatchDB<AppState>(source$)
 * db.start()
 *
 * // Type-safe deep watching (up to 6 levels)
 * db.watch$('settings', 'theme').subscribe(theme => console.log(theme))
 * ```
 */
export class PatchDB<T extends { [key: string]: any }> {
  private sub: Subscription | null = null
  protected watchedNodes: {
    [path: string]: {
      subject: BehaviorSubject<any>
      pathArr: string[]
    }
  } = {}

  /**
   * @param source$ - Observable delivering batches of updates from the server.
   * @param cache$ - Optional initial cache. Defaults to an empty document at revision 0.
   */
  constructor(
    private readonly source$: Observable<Update<T>[]>,
    protected readonly cache$ = new BehaviorSubject<Dump<T>>({
      id: 0,
      value: {} as T,
    }),
  ) {}

  /**
   * Begin listening to the source observable and applying updates.
   * Calling `start()` when already started is a no-op.
   */
  start() {
    if (this.sub) return

    // Simplified from `source$.pipe(withLatestFrom(cache$))`. Both are
    // equivalent since cache$ is a BehaviorSubject and processUpdates
    // mutates/re-emits synchronously, but `.value` is more direct.
    this.sub = this.source$.subscribe(updates => {
      this.processUpdates(updates, this.cache$.value)
    })
  }

  /**
   * Stop listening, complete all watched node subjects, and reset the cache.
   * Calling `stop()` when already stopped is a no-op.
   */
  stop() {
    if (!this.sub) return

    Object.values(this.watchedNodes).forEach(node => node.subject.complete())
    this.watchedNodes = {}
    this.sub.unsubscribe()
    this.sub = null
    this.cache$.next({ id: 0, value: {} as T })
  }

  /**
   * Returns an observable of the value at the given path within the document.
   *
   * Overloaded for 0–6 path segments with full type safety.
   * After the initial value, emits only when `value !== previousValue`.
   *
   * The observable waits for the first non-zero revision (i.e. a real dump)
   * before emitting, so subscribers won't see the empty initial state.
   *
   * @example
   * ```ts
   * // Watch the entire document
   * db.watch$().subscribe(state => ...)
   *
   * // Watch a nested path
   * db.watch$('users', 'abc123', 'name').subscribe(name => ...)
   * ```
   */
  // @claude fix #13: Removed outer NonNullable wrapper from the 1-level
  // overload return type. Runtime values can be null/undefined (e.g. after a
  // remove operation), so the previous NonNullable<T[P1]> was unsound — callers
  // skipped null checks based on the type, leading to runtime crashes.
  watch$(): Observable<T>
  watch$<P1 extends keyof T>(p1: P1): Observable<T[P1]>
  watch$<P1 extends keyof T, P2 extends keyof NonNullable<T[P1]>>(
    p1: P1,
    p2: P2,
  ): Observable<NonNullable<T[P1]>[P2]>
  watch$<
    P1 extends keyof T,
    P2 extends keyof NonNullable<T[P1]>,
    P3 extends keyof NonNullable<NonNullable<T[P1]>[P2]>,
  >(p1: P1, p2: P2, p3: P3): Observable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>
  watch$<
    P1 extends keyof T,
    P2 extends keyof NonNullable<T[P1]>,
    P3 extends keyof NonNullable<NonNullable<T[P1]>[P2]>,
    P4 extends keyof NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>,
  >(
    p1: P1,
    p2: P2,
    p3: P3,
    p4: P4,
  ): Observable<NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>[P4]>
  watch$<
    P1 extends keyof T,
    P2 extends keyof NonNullable<T[P1]>,
    P3 extends keyof NonNullable<NonNullable<T[P1]>[P2]>,
    P4 extends keyof NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>,
    P5 extends keyof NonNullable<
      NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>[P4]
    >,
  >(
    p1: P1,
    p2: P2,
    p3: P3,
    p4: P4,
    p5: P5,
  ): Observable<
    NonNullable<NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>[P4]>[P5]
  >
  watch$<
    P1 extends keyof T,
    P2 extends keyof NonNullable<T[P1]>,
    P3 extends keyof NonNullable<NonNullable<T[P1]>[P2]>,
    P4 extends keyof NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>,
    P5 extends keyof NonNullable<
      NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>[P4]
    >,
    P6 extends keyof NonNullable<
      NonNullable<NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>[P4]>[P5]
    >,
  >(
    p1: P1,
    p2: P2,
    p3: P3,
    p4: P4,
    p5: P5,
    p6: P6,
  ): Observable<
    NonNullable<
      NonNullable<NonNullable<NonNullable<NonNullable<T[P1]>[P2]>[P3]>[P4]>[P5]
    >[P6]
  >
  watch$(...args: (string | number)[]): Observable<any> {
    return this.cache$.pipe(
      filter(({ id }) => !!id),
      take(1),
      switchMap(({ value }) => {
        const path = pathFromArray(args)
        if (!this.watchedNodes[path]) {
          this.watchedNodes[path] = {
            subject: new BehaviorSubject(getValueByPointer(value, path)),
            pathArr: arrayFromPath(path),
          }
        }
        return this.watchedNodes[path].subject
      }),
    )
  }

  /**
   * Processes a batch of updates (dumps and/or revisions) against the cache.
   *
   * Revisions with an id below the expected next revision are skipped (deduplication).
   * Revisions that skip ahead (gap detected) are applied with a warning, since the
   * state may be inconsistent until the next full dump.
   *
   * After all updates are applied, the cache subject emits the new state.
   *
   * @param updates - The batch of updates to process.
   * @param cache - The current cache (mutated in place).
   */
  processUpdates(updates: Update<T>[], cache: Dump<T>) {
    updates.forEach(update => {
      if (this.isRevision(update)) {
        const expected = cache.id + 1
        if (update.id < expected) return
        this.handleRevision(update, cache)
      } else {
        this.handleDump(update, cache)
      }
      cache.id = update.id
    })
    this.cache$.next(cache)
  }

  /** @deprecated Use {@link processUpdates} instead. */
  proccessUpdates(updates: Update<T>[], cache: Dump<T>) {
    this.processUpdates(updates, cache)
  }

  protected handleRevision(revision: Revision, cache: Dump<T>): void {
    revision.patch.forEach(op => {
      applyOperation(cache, op)
    })
    this.publishRevision(revision, cache)
  }

  /** Watchers observe complete revisions; callers must apply every operation first. */
  protected publishRevision(revision: Revision, cache: Dump<T>): void {
    const patchArrs = revision.patch.map(op => ({
      path: op.path,
      arr: arrayFromPath(op.path),
    }))
    Object.entries(this.watchedNodes).forEach(([watchedPath, { pathArr }]) => {
      const match = patchArrs.find(
        ({ arr }) => startsWith(pathArr, arr) || startsWith(arr, pathArr),
      )
      if (match) this.updateWatchedNode(watchedPath, cache.value)
    })
  }

  /** Keeps the identity of every subtree the dump leaves unchanged. */
  protected handleDump(dump: Dump<T>, cache: Dump<T>): void {
    cache.value = reconcile(cache.value, dump.value ?? {}) as T
    Object.keys(this.watchedNodes).forEach(watchedPath => {
      this.updateWatchedNode(watchedPath, cache.value)
    })
  }

  /** Notifies the watch only when its value is a different object. */
  protected updateWatchedNode(path: string, data: T): void {
    const node = this.watchedNodes[path]
    const value = getValueByPointer(data, path)
    if (node && node.subject.value !== value) node.subject.next(value)
  }

  private isRevision(update: Update<T>): update is Revision {
    return 'patch' in update
  }
}

function startsWith(a: string[], b: string[]) {
  for (let i = 0; i < b.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}
