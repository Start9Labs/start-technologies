import { Effects } from '../Effects'
import { AbortedError } from './AbortedError'
import { deepEqual } from './deepEqual'
import { DropGenerator, DropPromise } from './Drop'

/** A reader `Watchable.from` and `Watchable.combine` can follow. */
export type WatchSource<A> = {
  once(): Promise<A>
  watch(abort?: AbortSignal): AsyncGenerator<A, unknown, unknown>
}

type WatchSources<V extends unknown[]> = { [K in keyof V]: WatchSource<V[K]> }

export abstract class Watchable<A> implements WatchSource<A> {
  /** A reader over a source, emitting when a value differs from the last by `eq`. */
  static from<A>(
    effects: Effects,
    source: WatchSource<A>,
    eq?: (a: A, b: A) => boolean,
  ): Watchable<A> {
    return new FromSource(effects, source, eq)
  }

  /**
   * A reader over several sources, whose raw value is the tuple of their
   * values. Its `watch`es end with this reader's.
   */
  static combine<V extends unknown[], Mapped = V>(
    effects: Effects,
    sources: readonly [...WatchSources<V>],
    map?: (values: V) => Mapped,
    eq?: (a: Mapped, b: Mapped) => boolean,
  ): Watchable<Mapped> {
    return new Combined(effects, sources, { map, eq })
  }

  protected readonly eqFn: (a: A, b: A) => boolean

  constructor(
    readonly effects: Effects,
    eq?: (a: A, b: A) => boolean,
  ) {
    this.eqFn = eq ?? ((a, b) => deepEqual(a, b))
  }

  /**
   * Fetch the current value, optionally registering a callback for change notification.
   * The callback should be invoked when the underlying data changes.
   */
  protected abstract fetch(callback?: () => void): Promise<A>
  protected abstract readonly label: string

  /**
   * Produce a stream of values. Default implementation uses fetch() with
   * effects callback in a loop. Override for custom subscription mechanisms
   * (e.g. fs.watch).
   */
  protected produce(abort: AbortSignal): AsyncGenerator<A, void> {
    return this.poll(abort, callback => this.fetch(callback))
  }

  /** Values from repeated fetches, each after the previous one's callback fires. */
  protected async *poll<T>(
    abort: AbortSignal,
    fetch: (callback: () => void) => Promise<T>,
  ): AsyncGenerator<T, void> {
    const resolveCell = { resolve: () => {} }
    this.effects.onLeaveContext(() => {
      resolveCell.resolve()
    })
    abort.addEventListener('abort', () => resolveCell.resolve())
    while (this.effects.isInContext && !abort.aborted) {
      let callback: () => void = () => {}
      const waitForNext = new Promise<void>(resolve => {
        callback = resolve
        resolveCell.resolve = resolve
      })
      yield await fetch(() => callback())
      await waitForNext
    }
  }

  /**
   * Lifecycle hook called when const() registers a subscription.
   * Return a cleanup function to be called when the subscription ends.
   * Override for side effects like FileHelper's consts tracking.
   */
  protected onConstRegistered(_value: A): (() => void) | void {}

  /**
   * Internal generator that deduplicates produced values using eq.
   */
  private async *watchGen(
    abort: AbortSignal,
  ): AsyncGenerator<A, void, unknown> {
    let prev: { value: A } | null = null
    for await (const value of this.produce(abort)) {
      if (abort.aborted) return
      if (!prev || !this.eqFn(prev.value, value)) {
        prev = { value }
        yield value
      }
    }
  }

  /**
   * Returns the value. Reruns the context from which it has been called if the underlying value changes
   */
  async const(): Promise<A> {
    const abort = new AbortController()
    const gen = this.watchGen(abort.signal)
    const res = await gen.next()
    const value = res.value as A
    if (this.effects.constRetry) {
      const constRetry = this.effects.constRetry
      const cleanup = this.onConstRegistered(value)
      gen.next().then(
        a => {
          abort.abort()
          cleanup?.()
          if (!a.done) {
            constRetry()
          }
        },
        e => {
          abort.abort()
          cleanup?.()
          console.error(
            `watch aborted, no longer reacting to changes @ ${this.label}.const`,
            e,
          )
        },
      )
    } else {
      abort.abort()
    }
    return value
  }

  /**
   * Returns the value. Does nothing if the value changes
   */
  async once(): Promise<A> {
    return this.fetch()
  }

  /**
   * Watches the value. Returns an async iterator that yields whenever the value changes
   */
  watch(abort?: AbortSignal): AsyncGenerator<A, never, unknown> {
    const ctrl = new AbortController()
    abort?.addEventListener('abort', () => ctrl.abort())
    return DropGenerator.of(
      (async function* (gen): AsyncGenerator<A, never, unknown> {
        yield* gen
        throw new AbortedError()
      })(this.watchGen(ctrl.signal)),
      () => ctrl.abort(),
    )
  }

  /**
   * Watches the value. Takes a custom callback function to run whenever the value changes
   */
  onChange(
    callback: (
      value: A | undefined,
      error?: Error,
    ) => { cancel: boolean } | Promise<{ cancel: boolean }>,
  ) {
    ;(async () => {
      const ctrl = new AbortController()
      for await (const value of this.watchGen(ctrl.signal)) {
        try {
          const res = await callback(value)
          if (res.cancel) {
            ctrl.abort()
            break
          }
        } catch (e) {
          console.error(
            `callback function threw an error @ ${this.label}.onChange`,
            e,
          )
        }
      }
    })()
      .catch(e => callback(undefined, e))
      .catch(e =>
        console.error(
          `callback function threw an error @ ${this.label}.onChange`,
          e,
        ),
      )
  }

  /**
   * Watches the value. Returns when the predicate is true
   */
  waitFor(pred: (value: A) => boolean): Promise<A> {
    const ctrl = new AbortController()
    return DropPromise.of(
      Promise.resolve().then(async () => {
        for await (const next of this.watchGen(ctrl.signal)) {
          if (pred(next)) {
            return next
          }
        }
        throw new AbortedError()
      }),
      () => ctrl.abort(),
    )
  }
}

/** A {@link Watchable} over a raw value, reading `map`'s result of it. */
export abstract class MappedWatchable<Raw, Mapped> extends Watchable<Mapped> {
  protected readonly mapFn: (value: Raw) => Mapped

  constructor(
    effects: Effects,
    options?: {
      map?: (value: Raw) => Mapped
      eq?: (a: Mapped, b: Mapped) => boolean
    },
  ) {
    super(effects, options?.eq)
    this.mapFn = options?.map ?? (a => a as unknown as Mapped)
  }

  /** Fetch the raw value, as {@link Watchable.fetch} does. */
  protected abstract fetchRaw(callback?: () => void): Promise<Raw>

  /** Produce raw values, as {@link Watchable.produce} does. */
  protected produceRaw(abort: AbortSignal): AsyncGenerator<Raw, void> {
    return this.poll(abort, callback => this.fetchRaw(callback))
  }

  protected async fetch(callback?: () => void) {
    return this.mapFn(await this.fetchRaw(callback))
  }

  protected async *produce(abort: AbortSignal): AsyncGenerator<Mapped, void> {
    for await (const raw of this.produceRaw(abort)) yield this.mapFn(raw)
  }
}

class FromSource<A> extends Watchable<A> {
  protected readonly label = 'Watchable.from'

  constructor(
    effects: Effects,
    private readonly source: WatchSource<A>,
    eq?: (a: A, b: A) => boolean,
  ) {
    super(effects, eq)
  }

  protected fetch() {
    return this.source.once()
  }

  protected async *produce(abort: AbortSignal): AsyncGenerator<A, void> {
    try {
      for await (const value of this.source.watch(abort)) yield value
    } catch (e) {
      if (!(e instanceof AbortedError)) throw e
    }
  }
}

class Combined<V extends unknown[], Mapped> extends MappedWatchable<V, Mapped> {
  protected readonly label = 'Watchable.combine'

  constructor(
    effects: Effects,
    private readonly sources: readonly [...WatchSources<V>],
    options: {
      map?: (values: V) => Mapped
      eq?: (a: Mapped, b: Mapped) => boolean
    },
  ) {
    super(effects, options)
  }

  protected async fetchRaw() {
    return (await Promise.all(this.sources.map(s => s.once()))) as V
  }

  protected async *produceRaw(abort: AbortSignal): AsyncGenerator<V, void> {
    const gens = this.sources.map(s => s.watch(abort))
    const next = (i: number) =>
      gens[i].next().then(
        r => ({ i, r }),
        e => {
          if (e instanceof AbortedError) return { i, r: null }
          throw e
        },
      )
    const first = await Promise.all(gens.map((_, i) => next(i)))
    const values: unknown[] = []
    for (const { i, r } of first) {
      if (!r || r.done) return
      values[i] = r.value
    }
    const pending = gens.map((_, i) => next(i))
    while (!abort.aborted) {
      yield [...values] as V
      const { i, r } = await Promise.race(pending)
      if (!r || r.done) return
      values[i] = r.value
      pending[i] = next(i)
    }
  }
}
