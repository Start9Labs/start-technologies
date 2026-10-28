import { ExtendedVersion, VersionRange } from '@start9labs/start-core/exver'
import * as T from '@start9labs/start-core/types'
import { IMPOSSIBLE, VersionInfo } from '../version/VersionInfo'
import { VersionGraph } from '../version/VersionGraph'

function store(initial: string | null) {
  let version = initial
  const effects = {
    getDataVersion: async () => version,
    setDataVersion: async ({ version: v }: { version: string | null }) => {
      version = v
      return null
    },
  } as unknown as T.Effects
  return { effects, read: () => version }
}

const ups: string[] = []

const knots = VersionGraph.of({
  current: VersionInfo.of<'#knots:29.3:1'>({
    version: '#knots:29.3:1',
    releaseNotes: '',
    migrations: {
      up: async () => {},
      down: IMPOSSIBLE,
      other: { ['^31']: { up: async () => {}, down: async () => {} } },
    },
  }),
  other: [],
})

const core = VersionGraph.of({
  current: VersionInfo.of<'31.1:19'>({
    version: '31.1:19',
    releaseNotes: '',
    migrations: {
      up: async () => {
        ups.push('31.1:19')
      },
      down: async () => {},
    },
  }),
  other: [],
})

beforeEach(() => {
  ups.length = 0
})

describe('data version after a migration that ends on a range', () => {
  test('a flavor switch stores the exact version being installed', async () => {
    const { effects, read } = store('#knots:29.3:1')
    await knots.uninit(effects, ExtendedVersion.parse('31.1:18'))
    expect(read()).toBe('31.1:18')
  })

  test('the next update in that series runs its migration', async () => {
    const { effects, read } = store('#knots:29.3:1')
    await knots.uninit(effects, ExtendedVersion.parse('31.1:18'))
    await core.init(effects)
    expect(ups).toEqual(['31.1:19'])
    expect(read()).toBe('31.1:19')
  })

  test('a range already stored settles on the current version', async () => {
    const { effects, read } = store(VersionRange.parse('^31:0').toString())
    await core.init(effects)
    expect(ups).toEqual([])
    expect(read()).toBe('31.1:19')
  })

  test('a range target is stored as the range', async () => {
    const { effects, read } = store('31.1:19')
    const target = VersionRange.parse('>=31:0 <31.1:19')
    await core.uninit(effects, target)
    expect(() => ExtendedVersion.parse(read()!)).toThrow()
  })
})
