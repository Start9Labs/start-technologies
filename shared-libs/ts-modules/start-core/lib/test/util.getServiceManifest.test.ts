import { Effects } from '../Effects'
import { getServiceManifest } from '../util/GetServiceManifest'

const manifest = { id: 'tor', version: '0.4.9.13:1' }

const makeEffects = (
  getServiceManifest: jest.Mock,
  getStatus: jest.Mock,
  constRetry?: () => void,
) =>
  ({
    isInContext: true,
    onLeaveContext: () => {},
    constRetry,
    getServiceManifest,
    getStatus,
  }) as unknown as Effects

describe('getServiceManifest', () => {
  test('reads the manifest of an installed package', async () => {
    const effects = makeEffects(
      jest.fn().mockResolvedValue(manifest),
      jest.fn(),
    )
    expect(
      await getServiceManifest(effects, 'tor', m => m?.version).once(),
    ).toBe('0.4.9.13:1')
  })

  test('is null for a package the OS rejects as not installed', async () => {
    const getStatus = jest.fn().mockResolvedValue(null)
    const effects = makeEffects(
      jest.fn().mockRejectedValue(new Error('missing stateInfo')),
      getStatus,
    )
    expect(await getServiceManifest(effects, 'tor').once()).toBeNull()
    expect(getStatus).toHaveBeenCalledWith(
      expect.objectContaining({ packageId: 'tor' }),
    )
  })

  test('const re-runs once the absent package is installed', async () => {
    let installed: (() => void) | undefined
    const constRetry = jest.fn()
    const read = jest.fn().mockRejectedValue(new Error('missing stateInfo'))
    const effects = makeEffects(
      read,
      jest.fn(async ({ callback }) => {
        installed = callback
        return null
      }),
      constRetry,
    )
    expect(await getServiceManifest(effects, 'tor').const()).toBeNull()
    read.mockResolvedValue(manifest)
    installed?.()
    await new Promise(r => setTimeout(r, 10))
    expect(constRetry).toHaveBeenCalledTimes(1)
  })

  test('rethrows for an installed package', async () => {
    const effects = makeEffects(
      jest.fn().mockRejectedValue(new Error('unreadable')),
      jest.fn().mockResolvedValue({ main: 'running' }),
    )
    await expect(getServiceManifest(effects, 'tor').once()).rejects.toThrow(
      'unreadable',
    )
  })
})
