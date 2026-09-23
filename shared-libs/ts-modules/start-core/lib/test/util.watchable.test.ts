import { Effects } from '../Effects'
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
  test('once() applies the function to each source', async () => {
    const effects = makeEffects()
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 'x')
    const sum = Watchable.combine(effects, [a, b], (n, s) => `${s}${n}`)
    expect(await sum.once()).toBe('x1')
  })

  test('const() re-runs the caller when either source changes', async () => {
    const constRetry = jest.fn()
    const effects = makeEffects(constRetry)
    const a = new Cell(effects, 1)
    const b = new Cell(effects, 2)
    expect(
      await Watchable.combine(effects, [a, b], (x, y) => x + y).const(),
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
    await Watchable.combine(effects, [a, b], (x, y) => x < y).const()
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
          (x, y) => x + y,
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
