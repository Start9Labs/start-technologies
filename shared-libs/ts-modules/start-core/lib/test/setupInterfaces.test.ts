import { setupServiceInterfaces } from '../interfaces/setupInterfaces'
import { Effects } from '../Effects'

describe('setupServiceInterfaces', () => {
  const fakeEffects = () => ({
    bind: jest.fn(async () => null),
    bindRange: jest.fn(async () => null),
    exportServiceInterface: jest.fn(async () => null),
    clearBindings: jest.fn(async () => null),
    clearServiceInterfaces: jest.fn(async () => null),
  })

  test.each([
    ['retireHost', (e: any) => e.retireHost({ id: 'old-ui' })],
    [
      'retireBinding',
      (e: any) => e.retireBinding({ id: 'ui', internalPort: 80 }),
    ],
  ])('%s throws inside a setupInterfaces pass', async (_name, call) => {
    const effects = fakeEffects()
    // The pass ends by disabling everything it did not declare, so a retire
    // here would depend on statement order.
    await expect(
      setupServiceInterfaces(async ({ effects }) => {
        await call(effects)
        return [] as any
      })(effects as unknown as Effects),
    ).rejects.toThrow('migration up()')
    expect(effects.clearBindings).not.toHaveBeenCalled()
  })

  test('a bindRange call is preserved in the clearBindings except set', async () => {
    const effects = fakeEffects()
    await setupServiceInterfaces(async ({ effects }) => {
      await effects.bindRange({
        id: 'turn',
        internalStartPort: 49152,
        externalStartPort: 49152,
        numberOfPorts: 100,
      })
      return [] as any
    })(effects as unknown as Effects)

    // The range was actually bound, and its start port made it into `except`
    // so the trailing clearBindings does not tear down the range the package
    // just requested on this same setup pass.
    expect(effects.bindRange).toHaveBeenCalledTimes(1)
    expect(effects.clearBindings).toHaveBeenCalledWith({
      except: [{ id: 'turn', internalPort: 49152 }],
    })
  })

  test('bind and bindRange are both tracked, in call order', async () => {
    const effects = fakeEffects()
    await setupServiceInterfaces(async ({ effects }) => {
      await effects.bind({ id: 'web', internalPort: 80 } as any)
      await effects.bindRange({
        id: 'turn',
        internalStartPort: 49152,
        externalStartPort: 49152,
        numberOfPorts: 10,
      })
      return [] as any
    })(effects as unknown as Effects)

    expect(effects.clearBindings).toHaveBeenCalledWith({
      except: [
        { id: 'web', internalPort: 80 },
        { id: 'turn', internalPort: 49152 },
      ],
    })
  })
})
