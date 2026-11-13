import { Effects } from '../Effects'
import { GrantedSharedVolume } from '../osBindings'
import { getSharedVolumes } from '../util/GetSharedVolumes'

function fixture() {
  let callback: (() => void) | undefined
  const volumes: GrantedSharedVolume[] = [
    {
      id: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567',
      name: 'Music',
      readonly: true,
      availability: 'available',
    },
  ]
  const constRetry = jest.fn()
  const read = jest.fn(async (options: { callback?: () => void }) => {
    callback = options.callback
    return structuredClone(volumes)
  })
  const effects = {
    isInContext: true,
    onLeaveContext: () => {},
    constRetry,
    getSharedVolumes: read,
  } as unknown as Effects
  return { effects, volumes, read, constRetry, invalidate: () => callback?.() }
}

describe('getSharedVolumes', () => {
  test('once fetches grants without subscribing', async () => {
    const f = fixture()
    expect(await getSharedVolumes(f.effects).once()).toEqual(f.volumes)
    expect(f.read).toHaveBeenCalledWith({ callback: undefined })
    f.invalidate()
    expect(f.constRetry).not.toHaveBeenCalled()
  })

  test('a filtered source invalidation retries const even when metadata stays equal', async () => {
    const f = fixture()
    expect(await getSharedVolumes(f.effects).const()).toEqual(f.volumes)
    await new Promise<void>(resolve => setImmediate(resolve))
    expect(f.constRetry).not.toHaveBeenCalled()
    f.invalidate()
    await new Promise<void>(resolve => setImmediate(resolve))
    expect(f.read).toHaveBeenCalledTimes(2)
    expect(f.constRetry).toHaveBeenCalledTimes(1)
  })

  test('an availability change still retries const', async () => {
    const f = fixture()
    await getSharedVolumes(f.effects).const()
    f.volumes[0].availability = 'unavailable'
    f.invalidate()
    await new Promise<void>(resolve => setImmediate(resolve))
    expect(f.constRetry).toHaveBeenCalledTimes(1)
  })
})
