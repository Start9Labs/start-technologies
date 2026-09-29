import { setFlagsFromString } from 'v8'
import { runInNewContext } from 'vm'
import { Effects } from '../Effects'
import { DropPromise } from '../util/Drop'
import { Watchable } from '../util/Watchable'

setFlagsFromString('--expose-gc')
const gc: () => void = runInNewContext('gc')

const collect = async () => {
  for (let i = 0; i < 5; i++) {
    gc()
    await new Promise(r => setTimeout(r, 10))
  }
}

class Never extends Watchable<number> {
  protected readonly label = 'Never'
  readonly signals: AbortSignal[] = []
  private readonly callbacks: (() => void)[] = []
  protected produce(abort: AbortSignal) {
    this.signals.push(abort)
    return super.produce(abort)
  }
  protected async fetch(callback?: () => void) {
    if (callback) this.callbacks.push(callback)
    return 0
  }
}

const effects = {
  isInContext: true,
  onLeaveContext: () => {},
} as unknown as Effects

describe('DropPromise under garbage collection', () => {
  test('an awaited waitFor keeps waiting', async () => {
    const never = new Never(effects)
    let settled = false
    ;(async () => {
      await never.waitFor(v => v > 0)
    })().finally(() => (settled = true))
    await collect()
    expect(never.signals[0].aborted).toBe(false)
    expect(settled).toBe(false)
  })

  test('a chained waitFor keeps waiting', async () => {
    const never = new Never(effects)
    let settled = false
    never
      .waitFor(v => v > 0)
      .then(
        () => (settled = true),
        () => (settled = true),
      )
    await collect()
    expect(never.signals[0].aborted).toBe(false)
    expect(settled).toBe(false)
  })

  test('a watch dropped between next calls aborts', async () => {
    const never = new Never(effects)
    await never.watch().next()
    await collect()
    expect(never.signals[0].aborted).toBe(true)
  })

  test('an unsubscribed DropPromise runs its drop', async () => {
    const drop = jest.fn()
    DropPromise.of(new Promise(() => {}), drop)
    await collect()
    expect(drop).toHaveBeenCalledTimes(1)
  })
})
