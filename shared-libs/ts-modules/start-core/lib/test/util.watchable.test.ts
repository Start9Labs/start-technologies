import { Effects } from '../Effects'
import { AbortedError } from '../util/AbortedError'
import { Watchable } from '../util/Watchable'

const tick = () => new Promise(r => setTimeout(r, 10))

class Cell<A> extends Watchable<A> {
  protected readonly label = 'Cell'
  private callbacks: (() => void)[] = []
  constructor(
    effects: Effects,
    private value: A,
  ) {
    super(effects)
  }
  set(value: A) {
    this.value = value
    for (const callback of this.callbacks.splice(0)) callback()
  }
  protected async fetch(callback?: () => void) {
    if (callback) this.callbacks.push(callback)
    return this.value
  }
}

const makeEffects = (constRetry?: () => void) =>
  ({
    isInContext: true,
    onLeaveContext: () => {},
    constRetry,
  }) as unknown as Effects

describe('Watchable.combine', () => {
  test('without map, reads the tuple of values', async () => {
    const effects = makeEffects()
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 'x')
    expect(await Watchable.combine(effects, [a, b]).once()).toEqual([1, 'x'])
  })

  test('eq decides what counts as a change', async () => {
    const constRetry = jest.fn()
    const effects = makeEffects(constRetry)
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 2)
    await Watchable.combine(
      effects,
      [a, b],
      ([x, y]) => x + y,
      (p, q) => p % 2 === q % 2,
    ).const()
    a.set(3)
    await tick()
    expect(constRetry).not.toHaveBeenCalled()
    b.set(3)
    await tick()
    expect(constRetry).toHaveBeenCalledTimes(1)
  })

  test('once() applies the function to each source', async () => {
    const effects = makeEffects()
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 'x')
    const sum = Watchable.combine(effects, [a, b], ([n, s]) => `${s}${n}`)
    expect(await sum.once()).toBe('x1')
  })

  test('const() re-runs the caller when either source changes', async () => {
    const constRetry = jest.fn()
    const effects = makeEffects(constRetry)
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 2)
    expect(
      await Watchable.combine(effects, [a, b], ([x, y]) => x + y).const(),
    ).toBe(3)
    b.set(5)
    await tick()
    expect(constRetry).toHaveBeenCalledTimes(1)
  })

  test('const() leaves the caller alone while the result holds', async () => {
    const constRetry = jest.fn()
    const effects = makeEffects(constRetry)
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 2)
    await Watchable.combine(effects, [a, b], ([x, y]) => x < y).const()
    a.set(0)
    await tick()
    expect(constRetry).not.toHaveBeenCalled()
  })

  test('watch() yields each new result until aborted', async () => {
    const effects = makeEffects()
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 10)
    const abort = new AbortController()
    const seen: number[] = []
    const done = (async () => {
      try {
        for await (const v of Watchable.combine(
          effects,
          [a, b],
          ([x, y]) => x + y,
        ).watch(abort.signal))
          seen.push(v)
      } catch {}
    })()
    await tick()
    a.set(2)
    await tick()
    b.set(20)
    await tick()
    abort.abort()
    a.set(3)
    await done
    expect(seen).toEqual([11, 12, 22])
  })
})

describe('Watchable.combine cleanup', () => {
  const held = () => {
    const signals: AbortSignal[] = []
    return {
      signals,
      once: async () => 0,
      watch: async function* (abort?: AbortSignal) {
        signals.push(abort!)
        yield 0
        await new Promise(r => abort!.addEventListener('abort', r))
      },
    }
  }
  const ending = {
    once: async () => 0,
    watch: async function* () {
      yield 0
    },
  }

  test('a source that ends stops the other sources', async () => {
    const effects = makeEffects()
    const b = held()
    await expect(
      (async () => {
        for await (const _ of Watchable.combine(effects, [ending, b]).watch());
      })(),
    ).rejects.toThrow(AbortedError)
    expect(b.signals[0].aborted).toBe(true)
  })

  test('leaving the loop stops every source', async () => {
    const effects = makeEffects()
    const b = held()
    for await (const _ of Watchable.combine(effects, [b, held()]).watch()) break
    expect(b.signals[0].aborted).toBe(true)
  })
})

describe('Watchable.waitFor', () => {
  test('rejects once the signal aborts', async () => {
    const effects = makeEffects()
    const abort = new AbortController()
    const wait = new Cell(effects, 0).waitFor(v => v > 0, abort.signal)
    await tick()
    abort.abort()
    await expect(wait).rejects.toThrow(AbortedError)
  })

  test('rejects on an already-aborted signal', async () => {
    const effects = makeEffects()
    await expect(
      new Cell(effects, 0).waitFor(v => v > 0, AbortSignal.abort()),
    ).rejects.toThrow(AbortedError)
  })

  test('resolves when the predicate holds', async () => {
    const effects = makeEffects()
    const cell = new Cell(effects, 0)
    const wait = cell.waitFor(v => v > 0, new AbortController().signal)
    await tick()
    cell.set(2)
    expect(await wait).toBe(2)
  })
})

describe('Watchable.from', () => {
  const source = <A>(values: A[]) => ({
    once: async () => values[0],
    watch: async function* (abort?: AbortSignal) {
      for (const v of values) {
        if (abort?.aborted) return
        yield v
        await tick()
      }
    },
  })

  test('once() reads the source once', async () => {
    const effects = makeEffects()
    expect(await Watchable.from(effects, source([1, 2])).once()).toBe(1)
  })

  test('watch() yields the source’s values, dropping repeats by eq', async () => {
    const effects = makeEffects()
    const seen: number[] = []
    await expect(
      (async () => {
        for await (const v of Watchable.from(
          effects,
          source([1, 1, 2, 3]),
        ).watch())
          seen.push(v)
      })(),
    ).rejects.toThrow(AbortedError)
    expect(seen).toEqual([1, 2, 3])
  })
})
