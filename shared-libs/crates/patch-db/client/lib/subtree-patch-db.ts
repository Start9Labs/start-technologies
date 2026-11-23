import {
  BehaviorSubject,
  distinctUntilChanged,
  EMPTY,
  filter,
  map,
  Observable,
  Subject,
  Subscription,
  switchMap,
  take,
} from 'rxjs'
import {
  arrayFromPath,
  getValueByPointer,
  Operation,
  pathFromArray,
} from './json-patch-lib'
import { PatchDB } from './patch-db'
import {
  mergeProjected,
  parseSubscriptionKey,
  Projection,
  projectRoot,
  subscriptionKey,
} from './projection'
import { Dump, PatchOp, Revision } from './types'

/** A revision's place in one server process's history. */
export type Cursor = { epoch: string; id: number }

/** The first event of a subtree stream: its value, or the revisions after the requested cursor. */
export type SubtreeStart = { epoch?: string | null; id: number } & (
  { value: unknown; revisions?: null } | { value?: null; revisions: Revision[] }
)

export type SubtreeEvent = SubtreeStart | Revision

export interface SubtreeSource {
  /** Streams the projected subtree at `path`: a start, then revisions scoped to `path`. */
  subscribe(
    path: string[],
    since: Cursor | null,
    projection: Projection | null,
  ): Observable<SubtreeEvent>
}

export type SubtreeOptions = {
  /** How long a path stays subscribed after its last watcher leaves. */
  releaseDelay?: number
  /** How long revisions from different subscriptions are collected before applying them in id order. */
  batch?: number
}

type Active = {
  path: string[]
  projection: Projection | null
  subscription: Subscription
  cursor: Cursor | null
}

/**
 * A {@link PatchDB} whose `watch$` subscriptions are server subscriptions.
 *
 * Each watched path and projection is reference-counted and released a short
 * delay after its last watcher leaves. An unprojected path serves every watch at
 * or below it, and a projected one serves only watches of the same projection;
 * only subscriptions no other watch serves are opened on the server. A projected
 * subscription writes the shared document only at paths its projection keeps.
 * Every subscription keeps its own cursor so it fast-forwards on reconnect, and
 * data from a released subscription stays readable until it is subscribed again.
 * The document cursor follows the lowest live subscription, including handovers.
 * A failed subscription stops them all and is reported on {@link failures$};
 * {@link start} resumes them.
 */
export class SubtreePatchDB<
  T extends { [key: string]: any },
> extends PatchDB<T> {
  private readonly watchers = new Map<string, number>()
  private readonly releases = new Map<string, ReturnType<typeof setTimeout>>()
  private readonly active = new Map<string, Active>()
  private readonly cursors = new Map<string, Cursor>()
  private readonly loaded = new Set<string>()
  private readonly changes$ = new BehaviorSubject<void>(undefined)
  private pending: { key: string; revision: Revision }[] = []
  private flush?: ReturnType<typeof setTimeout>
  private running = false
  private readonly releaseDelay: number
  private readonly batch: number

  /** Errors that stopped every subscription. */
  readonly failures$ = new Subject<unknown>()

  constructor(
    private readonly source: SubtreeSource,
    cache$ = new BehaviorSubject<Dump<T>>({ id: 0, value: {} as T }),
    { releaseDelay = 5_000, batch = 250 }: SubtreeOptions = {},
  ) {
    super(EMPTY, cache$)
    this.releaseDelay = releaseDelay
    this.batch = batch
  }

  /** Loads a persisted snapshot whose subscriptions resume from their cursors. */
  restore(dump: Dump<T>, cursors: Record<string, Cursor>) {
    this.cache$.next({ id: dump.id, value: { ...dump.value } })
    Object.entries(cursors).forEach(([key, cursor]) => {
      this.cursors.set(key, cursor)
      this.loaded.add(key)
    })
    Object.keys(this.watchedNodes).forEach(key =>
      this.updateWatchedNode(key, this.cache$.value.value),
    )
    this.changes$.next()
  }

  /** Cursors of every subscription whose data this document holds, by subscription key. */
  snapshotCursors(): Record<string, Cursor> {
    return Object.fromEntries(this.cursors)
  }

  override start() {
    if (this.running) return
    this.running = true
    this.reconcile()
  }

  override stop() {
    this.suspend()
    this.cursors.clear()
    this.loaded.clear()
    Object.values(this.watchedNodes).forEach(node => node.subject.complete())
    this.watchedNodes = {}
    this.releases.forEach(timer => clearTimeout(timer))
    this.releases.clear()
    this.watchers.forEach((count, key) => {
      if (!count) this.watchers.delete(key)
    })
    this.cache$.next({ id: 0, value: {} as T })
    this.changes$.next()
  }

  /** True while every path is covered by a live server subscription. */
  live$(...paths: (readonly string[])[]): Observable<boolean> {
    const keys = paths.map(path => pathFromArray([...path]))
    return this.changes$.pipe(
      map(() => this.running && keys.every(key => this.isLive(key))),
      distinctUntilChanged(),
    )
  }

  override watch$(...args: (string | number)[]): Observable<any> {
    return this.watchKey$(pathFromArray(args))
  }

  /** The projection of the value at a path, from a subscription narrowed to that projection. */
  watchProjection$(
    projection: Projection,
    ...args: (string | number)[]
  ): Observable<any> {
    return this.watchKey$(subscriptionKey(pathFromArray(args), projection))
  }

  protected override updateWatchedNode(key: string, data: T): void {
    this.watchedNodes[key]?.subject.next(read(key, data))
  }

  private watchKey$(key: string): Observable<any> {
    return new Observable(subscriber => {
      const release = this.acquire(key)
      const subscription = this.changes$
        .pipe(
          filter(() => this.covered(key)),
          take(1),
          switchMap(() => {
            this.watchedNodes[key] ??= {
              subject: new BehaviorSubject(read(key, this.cache$.value.value)),
              pathArr: arrayFromPath(parseSubscriptionKey(key).pointer),
            }
            return this.watchedNodes[key].subject
          }),
        )
        .subscribe(subscriber)
      return () => {
        subscription.unsubscribe()
        release()
      }
    })
  }

  private acquire(key: string): () => void {
    clearTimeout(this.releases.get(key))
    this.releases.delete(key)
    this.watchers.set(key, (this.watchers.get(key) ?? 0) + 1)
    this.reconcile()
    let released = false
    return () => {
      if (released) return
      released = true
      const count = (this.watchers.get(key) ?? 1) - 1
      this.watchers.set(key, count)
      if (count) return
      this.releases.set(
        key,
        setTimeout(() => {
          this.releases.delete(key)
          if (!this.watchers.get(key)) this.watchers.delete(key)
          this.reconcile()
        }, this.releaseDelay),
      )
    }
  }

  private roots(): string[] {
    const watched = [...this.watchers.keys()]
    return watched.filter(
      key => !watched.some(other => other !== key && serves(other, key)),
    )
  }

  private reconcile() {
    if (!this.running) return
    const roots = this.roots()
    roots.filter(key => !this.active.has(key)).forEach(key => this.open(key))
    ;[...this.active.keys()]
      .filter(key => !roots.includes(key))
      .forEach(key => {
        const server = roots.find(root => serves(root, key))
        if (server && !this.active.get(server)?.cursor) return
        this.close(key)
      })
  }

  /**
   * Replays from a cursor only when no other held subscription overlaps the data,
   * so a replay never re-applies revisions over newer data.
   */
  private sinceFor(key: string): Cursor | null {
    const overlapping = [...this.active.keys(), ...this.cursors.keys()].some(
      other => other !== key && overlaps(other, key) && !serves(other, key),
    )
    if (overlapping) return null
    const own = this.cursors.get(key)
    if (own) return own
    const ancestors = [...this.cursors.keys()]
      .filter(other => other !== key && serves(other, key))
      .sort((a, b) => b.length - a.length)
    return ancestors.length ? this.cursors.get(ancestors[0]!)! : null
  }

  private open(key: string) {
    const { pointer, projection } = parseSubscriptionKey(key)
    const path = arrayFromPath(pointer)
    const since = this.sinceFor(key)
    const entry: Active = {
      path,
      projection,
      cursor: null,
      subscription: new Subscription(),
    }
    this.active.set(key, entry)
    entry.subscription.add(
      this.source.subscribe(path, since, projection).subscribe({
        next: event => this.receive(key, event),
        error: error => this.fail(error),
        complete: () =>
          this.fail(new Error(`Subscription to ${pointer || '/'} ended`)),
      }),
    )
  }

  private close(key: string) {
    const entry = this.active.get(key)
    if (!entry) return
    this.active.delete(key)
    entry.subscription.unsubscribe()
    this.emit(this.cache$.value)
  }

  private suspend() {
    this.running = false
    clearTimeout(this.flush)
    this.flush = undefined
    this.applyPending()
    ;[...this.active.keys()].forEach(key => this.close(key))
  }

  private fail(error: unknown) {
    if (!this.running) return
    this.suspend()
    this.failures$.next(error)
  }

  private receive(key: string, event: SubtreeEvent) {
    const entry = this.active.get(key)
    if (!entry) return
    if ('patch' in event) {
      this.pending.push({ key, revision: event })
      this.flush ??= setTimeout(() => {
        this.flush = undefined
        this.applyPending()
      }, this.batch)
      return
    }
    const epoch = event.epoch ?? entry.cursor?.epoch ?? ''
    const cache = this.cache$.value
    if (event.revisions) {
      event.revisions.forEach(revision =>
        this.applyRevision(cache, entry, revision),
      )
    } else {
      this.applyAt(
        cache,
        entry.path,
        mergeProjected(
          entry.projection,
          [],
          getValueByPointer(cache.value, pathFromArray(entry.path)),
          event.value,
        ),
      )
    }
    entry.cursor = { epoch, id: event.id }
    ;[...this.cursors.keys()]
      .filter(other => other !== key && serves(key, other))
      .forEach(other => {
        this.cursors.delete(other)
        this.loaded.delete(other)
      })
    this.cursors.set(key, entry.cursor)
    this.loaded.add(key)
    this.emit(cache)
    this.reconcile()
  }

  private applyPending() {
    if (!this.pending.length) return
    const cache = this.cache$.value
    this.pending
      .splice(0)
      .sort((a, b) => a.revision.id - b.revision.id)
      .forEach(({ key, revision }) => {
        const entry = this.active.get(key)
        if (!entry?.cursor || revision.id <= entry.cursor.id) return
        this.applyRevision(cache, entry, revision)
        entry.cursor = { epoch: entry.cursor.epoch, id: revision.id }
        this.cursors.set(key, entry.cursor)
      })
    this.emit(cache)
  }

  /** Applies each operation against the document as the previous one left it. */
  private applyRevision(cache: Dump<T>, entry: Active, revision: Revision) {
    const pointer = pathFromArray(entry.path)
    revision.patch.forEach(op => {
      this.handleRevision(
        { id: revision.id, patch: [this.localize(cache, entry, pointer, op)] },
        cache,
      )
    })
  }

  private localize(
    cache: Dump<T>,
    { projection }: Active,
    pointer: string,
    op: Operation<unknown>,
  ): Operation<unknown> {
    const path = pointer + op.path
    if (op.op === PatchOp.REMOVE) return { ...op, path }
    if (!projection) {
      return op.path === ''
        ? this.replacementAt(cache, arrayFromPath(path), op.value)
        : { ...op, path }
    }
    const segments = arrayFromPath(path)
    const parent = getValueByPointer(
      cache.value,
      pathFromArray(segments.slice(0, -1)),
    )
    if (op.op === PatchOp.ADD && Array.isArray(parent)) return { ...op, path }
    return this.replacementAt(
      cache,
      segments,
      mergeProjected(
        projection,
        arrayFromPath(op.path),
        getValueByPointer(cache.value, path),
        op.value,
      ),
    )
  }

  private applyAt(cache: Dump<T>, path: string[], value: unknown) {
    if (!path.length) {
      this.handleDump({ id: cache.id, value: value as T }, cache)
      return
    }
    this.handleRevision(
      { id: cache.id, patch: [this.replacementAt(cache, path, value)] },
      cache,
    )
  }

  private replacementAt(
    cache: Dump<T>,
    path: string[],
    value: unknown,
  ): Operation<unknown> {
    const parents = path.slice(0, -1)
    const missing = parents.findIndex(
      (_, i) =>
        getValueByPointer(
          cache.value,
          pathFromArray(parents.slice(0, i + 1)),
        ) == null,
    )
    if (missing >= 0) {
      const at = parents.slice(0, missing + 1)
      const nested = path
        .slice(missing + 1)
        .reduceRight<unknown>((child, key) => ({ [key]: child }), value)
      return { op: PatchOp.REPLACE, path: pathFromArray(at), value: nested }
    }
    return { op: PatchOp.REPLACE, path: pathFromArray(path), value }
  }

  private emit(cache: Dump<T>) {
    const ids = [...this.active.values()].flatMap(({ cursor }) =>
      cursor ? [cursor.id] : [],
    )
    if (ids.length) cache.id = Math.min(...ids)
    this.cache$.next(cache)
    this.changes$.next()
  }

  private covered(key: string): boolean {
    return [...this.loaded].some(other => serves(other, key))
  }

  private isLive(key: string): boolean {
    return [...this.active.entries()].some(
      ([other, { cursor }]) => !!cursor && serves(other, key),
    )
  }
}

/** The watched value of a subscription key: its projection of the document at its pointer. */
function read(key: string, data: unknown): unknown {
  const { pointer, projection } = parseSubscriptionKey(key)
  return projectRoot(projection, getValueByPointer(data as any, pointer))
}

/**
 * Whether a subscription's data includes everything another subscription receives.
 * An unprojected subscription serves its subtree; a projected one only itself.
 */
function serves(server: string, served: string): boolean {
  if (server === served) return true
  const a = parseSubscriptionKey(server)
  if (a.projection) return false
  const b = parseSubscriptionKey(served)
  return a.pointer === b.pointer || isAncestor(a.pointer, b.pointer)
}

/** Whether two subscriptions' subtrees share any path. */
function overlaps(a: string, b: string): boolean {
  const p = parseSubscriptionKey(a).pointer
  const q = parseSubscriptionKey(b).pointer
  return p === q || isAncestor(p, q) || isAncestor(q, p)
}

/** True when `ancestor` is a proper ancestor of `pointer`. */
function isAncestor(ancestor: string, pointer: string): boolean {
  return ancestor === '' ? pointer !== '' : pointer.startsWith(ancestor + '/')
}
