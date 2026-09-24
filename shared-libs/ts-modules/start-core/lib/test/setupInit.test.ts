import { Effects } from '../Effects'
import { InitKind, setupInit } from '../inits/setupInit'
import { FullProgressTracker } from '../util/FullProgressTracker'

const flush = () => new Promise(resolve => setImmediate(resolve))

describe('setupInit reactive retries', () => {
  test.each<InitKind>(['install', 'update', 'restore', null])(
    'passes %s on the first run and null on retries',
    async initialKind => {
      const seen: Array<{ kind: InitKind; progress: FullProgressTracker }> = []
      const child = {
        constRetry: undefined as undefined | (() => Promise<void>),
      }
      const effects = {
        child: jest.fn(() => child),
        setInitProgress: jest.fn(async () => {}),
      } as unknown as Effects

      await setupInit(async (_effects, kind, progress) => {
        seen.push({ kind, progress })
      })({ effects, kind: initialKind })

      expect(seen.map(run => run.kind)).toEqual([initialKind])
      expect(child.constRetry).toBeDefined()
      await child.constRetry!()
      await flush()
      await child.constRetry!()
      await flush()

      expect(seen.map(run => run.kind)).toEqual([initialKind, null, null])
      expect(seen[1]!.progress).not.toBe(seen[0]!.progress)
      expect(seen[2]!.progress).not.toBe(seen[1]!.progress)
    },
  )

  it('passes null on an object init retry', async () => {
    const kinds: InitKind[] = []
    const child = { constRetry: undefined as undefined | (() => Promise<void>) }
    const effects = {
      child: () => child,
      setInitProgress: jest.fn(async () => {}),
    } as unknown as Effects

    await setupInit({
      init: async (_effects, kind) => {
        kinds.push(kind)
      },
    })({ effects, kind: 'restore' })
    await child.constRetry!()
    await flush()

    expect(kinds).toEqual(['restore', null])
  })
})
