import { Effects } from '../Effects'
import { AbortedError } from './AbortedError'
import { deepEqual } from './deepEqual'
import { DropGenerator, DropPromise } from './Drop'

export abstract class Watchable<Raw, Mapped = Raw> {
  protected readonly mapFn: (value: Raw) => Mapped
  protected readonly eqFn: (a: Mapped, b: Mapped) => boolean

  constructor(
    readonly effects: Effects,
    options?: {
      map?: (value: Raw) => Mapped
      eq?: (a: Mapped, b: Mapped) => boolean
    },
  ) {
    this.mapFn = options?.map ?? (a => a as unknown as Mapped)
    this.eqFn = options?.eq ?? ((a, b) => deepEqual(a, b))
  }

  /**
   * Fetch the current value, optionally registering a callback for change notification.
   * The callback should be invoked when the underlying data changes.
   */
  protected abstract fetch(callback?: () => void): Promise<Raw>
  protected abstract readonly label: string

  /**
   * Produce a stream of raw values. Default implementation uses fetch() with
   * effects callback in a loop. Override for custom subscription mechanisms
   * (e.g. fs.watch).
   */
  protected async *produce(abort: AbortSignal): AsyncGenerator<Raw, void> {
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
      yield await this.fetch(() => callback())
      await waitForNext
    }
  }

  /**
   * Lifecycle hook called when const() registers a subscription.
   * Return a cleanup function to be called when the subscription ends.
   * Override for side effects like FileHelper's consts tracking.
   */
  protected onConstRegistered(_value: Mapped): (() => void) | void {}

  /**
   * Internal generator that maps raw values and deduplicates using eq.
   */
  private async *watchGen(
    abort: AbortSignal,
  ): AsyncGenerator<Mapped, void, unknown> {
    let prev: { value: Mapped } | null = null
    for await (const raw of this.produce(abort)) {
      if (abort.aborted) return
      const mapped = this.mapFn(raw)
      if (!prev || !this.eqFn(prev.value, mapped)) {
        prev = { value: mapped }
        yield mapped
      }
    }
  }

  /**
   * Returns the value. Reruns the context from which it has been called if the underlying value changes
   */
  async const(): Promise<Mapped> {
    const abort = new AbortController()
    const gen = this.watchGen(abort.signal)
    const res = await gen.next()
    const value = res.value as Mapped
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
  async once(): Promise<Mapped> {
    return this.mapFn(await this.fetch())
  }

  /**
   * Watches the value. Returns an async iterator that yields whenever the value changes
   */
  watch(abort?: AbortSignal): AsyncGenerator<Mapped, never, unknown> {
    const ctrl = new AbortController()
    abort?.addEventListener('abort', () => ctrl.abort())
    return DropGenerator.of(
      (async function* (gen): AsyncGenerator<Mapped, never, unknown> {
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
      value: Mapped | undefined,
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
  waitFor(pred: (value: Mapped) => boolean): Promise<Mapped> {
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
