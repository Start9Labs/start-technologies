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
  applyOperation,
  arrayFromPath,
  getValueByPointer,
  Operation,
  pathFromArray,
} from './json-patch-lib'
import { PatchDB } from './patch-db'
import {
  minimalSelection,
  NOTHING,
  parseSelectorKey,
  parseSubscriptionKey,
  Projection,
  projectNarrowed,
  projectOperations,
  projectRoot,
  Selection,
  selectionContains,
  selectionKey,
  selectorKey,
  subscriptionKey,
  watchSelection,
  watchSelectors,
} from './projection'
import { Dump, Revision } from './types'

/** A revision's place in one server process's history. */
export type Cursor = { epoch: string; id: number }

/**
 * The first event of the subscription: its value, or the revisions after the requested
 * cursor, and the name `SubtreeSource.update` takes.
 */
export type SubtreeStart = {
  epoch?: string | null
  id: number
  subscription?: string | null
} & (
  | { value: unknown; revisions?: null }
  | { value?: null; revisions: Revision[] }
)

/** A projected revision; with `extend`, what the latest selection change added. */
export type SubtreeRevision = Revision & { extend?: boolean | null }

export type SubtreeEvent = SubtreeStart | SubtreeRevision

export interface SubtreeSource {
  /** Streams the selection of the subtree at `path`: a start, then revisions. */
  subscribe(
    path: string[],
    since: Cursor | null,
    selection: Selection,
  ): Observable<SubtreeEvent>
  /**
   * Replaces the subscription's selection; the stream then sends one `extend` revision with
   * what it adds, before any later revision.
   */
  update(subscription: string, selection: Selection): Promise<void>
}

export type SubtreeOptions = {
  /** How long a watch's data stays subscribed after its last watcher leaves. */
  releaseDelay?: number
  /** How long revisions are collected before they apply together. */
  batch?: number
}

/**
 * A {@link PatchDB} whose `watch$` calls select what one server subscription sends.
 *
 * Each watch is reference-counted and released a short delay after its last watcher
 * leaves. The document is a replica of the server's under the requested selection, which
 * is the union of the watches' selectors. A selection change prunes the replica at once
 * and fills it from the stream's `extend` revision, which also confirms the change. Pruning
 * keeps the identity of every subtree it leaves unchanged and notifies only the watches
 * whose subtree changed; widening notifies none. A watch is served once the confirmed
 * selection covers it. Selected array positions remain
 * correct across selection changes and preceding revisions. Selection updates remain
 * serialized through synchronous watcher callbacks. The document cursor is the
 * subscription's. A failed subscription is reported on {@link failures$}; {@link start}
 * resumes it from the cursor.
 */
export class SubtreePatchDB<
  T extends { [key: string]: any },
> extends PatchDB<T> {
  private readonly watchers = new Map<string, number>()
  private readonly releases = new Map<string, ReturnType<typeof setTimeout>>()
  private readonly changes$ = new BehaviorSubject<void>(undefined)
  private stream?: Subscription
  private subscription: string | null = null
  private cursor: Cursor | null = null
  /** The selection the document holds. */
  private held: Selection | null = null
  /** Confirmed coverage within `held`; empty coverage uses `NOTHING`, never `[]`. */
  private confirmed: Selection | null = null
  private updating: Selection | null = null
  /** Revisions preceding the extension still use the previous selection. */
  private beforeUpdate?: Dump<T>
  private pending: Revision[] = []
  private flush?: ReturnType<typeof setTimeout>
  private running = false
  private readonly releaseDelay: number
  private readonly batch: number

  /** Errors that stopped the subscription. */
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

  /** Loads a persisted snapshot: the selection it holds, keyed with its cursor. */
  restore(dump: Dump<T>, cursors: Record<string, Cursor>) {
    const entries = Object.entries(cursors)
    const [first] = entries
    const agreed =
      !!first &&
      entries.every(
        ([, c]) => c.epoch === first[1].epoch && c.id === first[1].id,
      )
    this.cache$.next({ id: dump.id, value: { ...dump.value } })
    if (agreed) {
      this.held = minimalSelection(entries.map(([key]) => parseSelectorKey(key)))
      this.confirmed = this.held
      this.cursor = first[1]
    }
    Object.keys(this.watchedNodes).forEach(key =>
      this.updateWatchedNode(key, this.cache$.value.value),
    )
    this.changes$.next()
  }

  /** The cursor of each selector the document holds, all one cursor. */
  snapshotCursors(): Record<string, Cursor> {
    const { cursor, confirmed } = this
    return cursor && confirmed
      ? Object.fromEntries(confirmed.map(s => [selectorKey(s), cursor]))
      : {}
  }

  override start() {
    if (this.running) return
    this.running = true
    this.open()
  }

  override stop() {
    this.suspend()
    this.cursor = null
    this.held = null
    this.confirmed = null
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

  /** True while the live subscription covers every path. */
  live$(...paths: (readonly string[])[]): Observable<boolean> {
    const keys = paths.map(path => pathFromArray([...path]))
    return this.changes$.pipe(
      map(
        () =>
          this.running &&
          !!this.subscription &&
          keys.every(key => this.covered(key)),
      ),
      distinctUntilChanged(),
    )
  }

  override watch$(...args: (string | number)[]): Observable<any> {
    return this.watchKey$(pathFromArray(args))
  }

  /** The projection of the value at a path, subscribing to that projection only. */
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
    this.reselect()
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
          this.reselect()
        }, this.releaseDelay),
      )
    }
  }

  /** The union of every watch's selectors; nothing without watches. */
  private desired(): Selection {
    const selection = minimalSelection(
      [...this.watchers.keys()].flatMap(watchSelectors),
    )
    return selection.length ? selection : NOTHING
  }

  private open() {
    const selection = this.held ?? this.desired()
    const since = this.held ? this.cursor : null
    this.held = selection
    this.stream = this.source.subscribe([], since, selection).subscribe({
      next: event => this.receive(event, selection),
      error: error => this.fail(error),
      complete: () => this.fail(new Error('Database subscription ended')),
    })
  }

  private reselect() {
    const subscription = this.subscription
    if (!this.running || !subscription || this.updating || !this.held) return
    const desired = this.desired()
    if (selectionKey(desired) === selectionKey(this.held)) return
    if (this.pending.length) {
      this.applyPending()
      this.reselect()
      return
    }
    this.beforeUpdate = { ...this.cache$.value }
    const held = this.held
    this.held = desired
    const confirmed = this.confirmed ?? NOTHING
    const retained = minimalSelection([
      ...confirmed.filter(s => selectionContains(desired, s)),
      ...desired.filter(s => selectionContains(confirmed, s)),
    ])
    this.confirmed = retained.length ? retained : NOTHING
    this.updating = desired
    const cache = this.cache$.value
    this.replaceValue(
      cache,
      projectNarrowed(
        desired,
        held.filter(s => !selectionContains(desired, s)),
        cache.value,
      ) as T | null,
    )
    this.emit(cache)
    this.source.update(subscription, desired).catch(error => {
      if (this.subscription === subscription) this.fail(error)
    })
  }

  /** Notifies the watches whose subtree the new value replaces. */
  private replaceValue(cache: Dump<T>, value: T | null) {
    const before = cache.value
    cache.value = value ?? ({} as T)
    Object.entries(this.watchedNodes).forEach(([key, { pathArr }]) => {
      const path = pathFromArray(pathArr)
      if (
        getValueByPointer(before, path) !== getValueByPointer(cache.value, path)
      )
        this.updateWatchedNode(key, cache.value)
    })
  }

  private receive(event: SubtreeEvent, selection: Selection) {
    if (!('patch' in event)) {
      const cache = this.cache$.value
      if (event.revisions) {
        event.revisions.forEach(revision => this.apply(cache, revision))
      } else {
        this.handleDump(
          { id: event.id, value: (event.value ?? {}) as T },
          cache,
        )
      }
      this.cursor = {
        epoch: event.epoch ?? this.cursor?.epoch ?? '',
        id: event.id,
      }
      this.subscription = event.subscription ?? null
      this.confirmed = selection
      this.emit(cache)
      this.reselect()
      return
    }
    if (event.extend) {
      this.applyPending()
      this.beforeUpdate = undefined
      const cache = this.cache$.value
      this.apply(cache, event, false)
      this.confirmed = this.updating
      this.updating = null
      this.emit(cache)
      this.reselect()
      return
    }
    this.pending.push(event)
    this.flush ??= setTimeout(() => {
      this.flush = undefined
      this.applyPending()
    }, this.batch)
  }

  private applyPending() {
    clearTimeout(this.flush)
    this.flush = undefined
    if (!this.pending.length) return
    const cache = this.cache$.value
    this.pending.splice(0).forEach(revision => this.apply(cache, revision))
    this.emit(cache)
  }

  /** Extensions are already projected. */
  private apply(cache: Dump<T>, revision: Revision, project = true) {
    if (this.cursor && revision.id > this.cursor.id)
      this.cursor = { ...this.cursor, id: revision.id }
    const target = (project && this.beforeUpdate) || cache
    const patch =
      project && target === cache && this.held
        ? projectOperations(revision.patch, this.held, path =>
            Array.isArray(getValueByPointer(cache.value, pathFromArray(path))),
          )
        : revision.patch
    patch.forEach(op => applyOperation(target, op as Operation<unknown>))
    if (target === cache) {
      this.publishRevision({ id: revision.id, patch }, cache)
    } else {
      this.handleDump(
        { id: revision.id, value: projectRoot(this.held!, target.value) as T },
        cache,
      )
    }
  }

  private suspend() {
    this.running = false
    this.applyPending()
    this.beforeUpdate = undefined
    this.stream?.unsubscribe()
    this.stream = undefined
    this.subscription = null
    if (this.updating) {
      this.updating = null
      this.held = this.confirmed
      const cache = this.cache$.value
      if (this.held)
        this.handleDump(
          { id: cache.id, value: projectRoot(this.held, cache.value) as T },
          cache,
        )
      this.cache$.next(cache)
    }
    this.changes$.next()
  }

  private fail(error: unknown) {
    if (!this.running) return
    this.suspend()
    this.failures$.next(error)
  }

  private emit(cache: Dump<T>) {
    if (this.cursor) cache.id = this.cursor.id
    this.cache$.next(cache)
    this.changes$.next()
  }

  private covered(key: string): boolean {
    const confirmed = this.confirmed
    return (
      !!confirmed &&
      watchSelectors(key).every(s => selectionContains(confirmed, s))
    )
  }
}

/** The watched value of a watch key: its projection of the document at its pointer. */
function read(key: string, data: unknown): unknown {
  const { pointer, projection } = parseSubscriptionKey(key)
  return projectRoot(
    watchSelection(projection),
    getValueByPointer(data as any, pointer),
  )
}
