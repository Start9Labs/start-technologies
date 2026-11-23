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
import { Dump, PatchOp, Revision } from './types'

/** A revision's place in one server process's history. */
export type Cursor = { epoch: string; id: number }

/** The first event of a subtree stream: its value, or the revisions after the requested cursor. */
export type SubtreeStart = { epoch?: string | null; id: number } & (
  { value: unknown; revisions?: null } | { value?: null; revisions: Revision[] }
)

export type SubtreeEvent = SubtreeStart | Revision

export interface SubtreeSource {
  /** Streams the subtree at `path`: a start, then revisions scoped to `path`. */
  subscribe(path: string[], since: Cursor | null): Observable<SubtreeEvent>
}

export type SubtreeOptions = {
  /** How long a path stays subscribed after its last watcher leaves. */
  releaseDelay?: number
  /** How long revisions from different subscriptions are collected before applying them in id order. */
  batch?: number
}

type Active = {
  path: string[]
  subscription: Subscription
  cursor: Cursor | null
}

/**
 * A {@link PatchDB} whose `watch$` subscriptions are server subscriptions.
 *
 * Each watched path is reference-counted; the outermost watched paths are
 * subscribed on the server, and a path is released a short delay after its last
 * watcher leaves. Every subscribed path keeps its own cursor so it fast-forwards
 * on reconnect, and data from a released path stays readable until it is
 * subscribed again. A failed subscription stops them all and is reported on
 * {@link failures$}; {@link start} resumes them.
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
  private pending: { pointer: string; revision: Revision }[] = []
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

  /** Loads a persisted snapshot whose subscribed paths resume from their cursors. */
  restore(dump: Dump<T>, cursors: Record<string, Cursor>) {
    this.cache$.next({ id: dump.id, value: { ...dump.value } })
    Object.entries(cursors).forEach(([pointer, cursor]) => {
      this.cursors.set(pointer, cursor)
      this.loaded.add(pointer)
    })
    Object.keys(this.watchedNodes).forEach(path =>
      this.updateWatchedNode(path, this.cache$.value.value),
    )
    this.changes$.next()
  }

  /** Cursors of every path whose data this document holds. */
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
    this.releases.forEach(timer => clearTimeout(timer))
    this.releases.clear()
    this.cursors.clear()
    this.loaded.clear()
    Object.values(this.watchedNodes).forEach(node => node.subject.complete())
    this.watchedNodes = {}
    this.cache$.next({ id: 0, value: {} as T })
    this.changes$.next()
  }

  /** True while every path is covered by a live server subscription. */
  live$(...paths: (readonly string[])[]): Observable<boolean> {
    const pointers = paths.map(path => pathFromArray([...path]))
    return this.changes$.pipe(
      map(() => this.running && pointers.every(p => this.isLive(p))),
      distinctUntilChanged(),
    )
  }

  override watch$(...args: (string | number)[]): Observable<any> {
    const path = pathFromArray(args)
    return new Observable(subscriber => {
      const release = this.acquire(path)
      const subscription = this.changes$
        .pipe(
          filter(() => this.covered(path)),
          take(1),
          switchMap(() => {
            this.watchedNodes[path] ??= {
              subject: new BehaviorSubject(
                getValueByPointer(this.cache$.value.value, path),
              ),
              pathArr: arrayFromPath(path),
            }
            return this.watchedNodes[path].subject
          }),
        )
        .subscribe(subscriber)
      return () => {
        subscription.unsubscribe()
        release()
      }
    })
  }

  private acquire(pointer: string): () => void {
    clearTimeout(this.releases.get(pointer))
    this.releases.delete(pointer)
    this.watchers.set(pointer, (this.watchers.get(pointer) ?? 0) + 1)
    this.reconcile()
    let released = false
    return () => {
      if (released) return
      released = true
      const count = (this.watchers.get(pointer) ?? 1) - 1
      this.watchers.set(pointer, count)
      if (count) return
      this.releases.set(
        pointer,
        setTimeout(() => {
          this.releases.delete(pointer)
          if (!this.watchers.get(pointer)) this.watchers.delete(pointer)
          this.reconcile()
        }, this.releaseDelay),
      )
    }
  }

  private roots(): string[] {
    const watched = [...this.watchers.keys()]
    return watched.filter(
      pointer => !watched.some(other => isAncestor(other, pointer)),
    )
  }

  private reconcile() {
    if (!this.running) return
    const roots = this.roots()
    roots
      .filter(pointer => !this.active.has(pointer))
      .forEach(pointer => this.open(pointer))
    ; [...this.active.keys()]
      .filter(pointer => !roots.includes(pointer))
      .forEach(pointer => {
        const ancestor = roots.find(root => isAncestor(root, pointer))
        if (ancestor && !this.active.get(ancestor)?.cursor) return
        this.close(pointer)
      })
  }

  private sinceFor(pointer: string): Cursor | null {
    const below = (other: string) => isAncestor(pointer, other)
    if ([...this.active.keys(), ...this.cursors.keys()].some(below)) return null
    const own = this.cursors.get(pointer)
    if (own) return own
    const ancestors = [...this.cursors.keys()]
      .filter(other => isAncestor(other, pointer))
      .sort((a, b) => b.length - a.length)
    return ancestors.length ? this.cursors.get(ancestors[0]!)! : null
  }

  private open(pointer: string) {
    const path = arrayFromPath(pointer)
    const since = this.sinceFor(pointer)
    const entry: Active = {
      path,
      cursor: null,
      subscription: new Subscription(),
    }
    this.active.set(pointer, entry)
    entry.subscription.add(
      this.source.subscribe(path, since).subscribe({
        next: event => this.receive(pointer, event),
        error: error => this.fail(error),
        complete: () =>
          this.fail(new Error(`Subscription to ${pointer || '/'} ended`)),
      }),
    )
  }

  private close(pointer: string) {
    const entry = this.active.get(pointer)
    if (!entry) return
    this.active.delete(pointer)
    entry.subscription.unsubscribe()
    this.changes$.next()
  }

  private suspend() {
    this.running = false
    clearTimeout(this.flush)
    this.flush = undefined
    this.applyPending()
    ; [...this.active.keys()].forEach(pointer => this.close(pointer))
  }

  private fail(error: unknown) {
    if (!this.running) return
    this.suspend()
    this.failures$.next(error)
  }

  private receive(pointer: string, event: SubtreeEvent) {
    const entry = this.active.get(pointer)
    if (!entry) return
    if ('patch' in event) {
      this.pending.push({ pointer, revision: event })
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
        this.applyRevision(cache, pointer, revision),
      )
    } else {
      this.applyAt(cache, entry.path, event.value)
    }
    entry.cursor = { epoch, id: event.id }
    ; [...this.cursors.keys()]
      .filter(other => isAncestor(pointer, other))
      .forEach(other => {
        this.cursors.delete(other)
        this.loaded.delete(other)
      })
    this.cursors.set(pointer, entry.cursor)
    this.loaded.add(pointer)
    this.emit(cache)
    this.reconcile()
  }

  private applyPending() {
    if (!this.pending.length) return
    const cache = this.cache$.value
    this.pending
      .splice(0)
      .sort((a, b) => a.revision.id - b.revision.id)
      .forEach(({ pointer, revision }) => {
        const entry = this.active.get(pointer)
        if (!entry?.cursor || revision.id <= entry.cursor.id) return
        this.applyRevision(cache, pointer, revision)
        entry.cursor = { epoch: entry.cursor.epoch, id: revision.id }
        this.cursors.set(pointer, entry.cursor)
      })
    this.emit(cache)
  }

  private applyRevision(cache: Dump<T>, pointer: string, revision: Revision) {
    if (!revision.patch.length) return
    this.handleRevision(
      {
        id: revision.id,
        patch: revision.patch.map(
          op => ({ ...op, path: pointer + op.path }) as Operation<unknown>,
        ),
      },
      cache,
    )
  }

  private applyAt(cache: Dump<T>, path: string[], value: unknown) {
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
      this.handleRevision(
        {
          id: cache.id,
          patch: [
            { op: PatchOp.REPLACE, path: pathFromArray(at), value: nested },
          ],
        },
        cache,
      )
    } else if (path.length) {
      this.handleRevision(
        {
          id: cache.id,
          patch: [{ op: PatchOp.REPLACE, path: pathFromArray(path), value }],
        },
        cache,
      )
    } else {
      this.handleDump({ id: cache.id, value: value as T }, cache)
    }
  }

  private emit(cache: Dump<T>) {
    const ids = [...this.active.values()].flatMap(({ cursor }) =>
      cursor ? [cursor.id] : [],
    )
    if (ids.length) cache.id = Math.min(...ids)
    this.cache$.next(cache)
    this.changes$.next()
  }

  private covered(pointer: string): boolean {
    return [...this.loaded].some(
      other => other === pointer || isAncestor(other, pointer),
    )
  }

  private isLive(pointer: string): boolean {
    return [...this.active.entries()].some(
      ([other, { cursor }]) =>
        !!cursor && (other === pointer || isAncestor(other, pointer)),
    )
  }
}

/** True when `ancestor` is a proper ancestor of `pointer`. */
function isAncestor(ancestor: string, pointer: string): boolean {
  return ancestor === '' ? pointer !== '' : pointer.startsWith(ancestor + '/')
}
