import { Effects } from '@start9labs/start-core/Effects'
import { onLeaveUntil } from '../util/onLeaveUntil'

describe('onLeaveUntil', () => {
  it('arms one hook per context and runs only unreleased callbacks', () => {
    const hooks: (() => void | null | undefined)[] = []
    const effects = {
      isInContext: true,
      onLeaveContext: (fn: () => void | null | undefined) => {
        hooks.push(fn)
      },
    } as Effects
    const released = jest.fn()
    const kept = jest.fn()

    onLeaveUntil(effects, released)()
    onLeaveUntil(effects, kept)
    expect(hooks).toHaveLength(1)

    for (const hook of hooks) hook()
    expect(released).not.toHaveBeenCalled()
    expect(kept).toHaveBeenCalledTimes(1)
  })
})
