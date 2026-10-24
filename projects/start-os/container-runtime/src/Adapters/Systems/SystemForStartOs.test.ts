import { T } from '@start9labs/start-sdk'
import { System } from '../../Interfaces/System'
import { SystemForStartOs } from './SystemForStartOs'

describe('StartOS backup results', () => {
  test.each([{ changedBytes: 1234 }, { changedBytes: null }, undefined])(
    'preserves the package result %p',
    async result => {
      const createBackup: T.ExpectedExports.createBackup = jest.fn(
        async () => result,
      )
      const system: System = new SystemForStartOs({ createBackup } as T.ABI)
      const effects = {} as T.Effects

      await expect(system.createBackup(effects, null)).resolves.toEqual(result)
      expect(createBackup).toHaveBeenCalledWith({ effects })
    },
  )

  test('accepts custom hooks returning void', async () => {
    const createBackup = async (): Promise<void> => {}
    const hook: T.ExpectedExports.createBackup = createBackup
    const system: System = new SystemForStartOs({ createBackup: hook } as T.ABI)

    await expect(
      system.createBackup({} as T.Effects, null),
    ).resolves.toBeUndefined()
  })
})
