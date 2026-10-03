import { RPC, T } from '../index'

describe('custom wire representations', () => {
  test('log boot selectors accept omission and null', () => {
    const omitted: T.LogsParamsInput = {}
    const request: RPC.RpcParamType<RPC.StartOS, 'server.logs'> = { boot: null }
    const output: T.LogsParams = {
      limit: null,
      cursor: null,
      boot: null,
      before: false,
    }
    // @ts-expect-error Boot identifiers are numeric or textual, not boolean.
    const invalid: T.LogsParamsInput = { boot: false }
    expect(omitted).toEqual({})
    expect(request.boot).toBeNull()
    expect(output.boot).toBeNull()
    expect(invalid.boot).toBe(false)
  })

  test('image configuration inputs include the legacy decoder contract', () => {
    const omitted: T.ImageConfigInput = { source: 'packed', arch: ['x86_64'] }
    const nullable: T.ImageConfigInput = {
      ...omitted,
      emulateMissing: null,
      emulateMissingAs: null,
    }
    const legacy: T.ImageConfigInput = {
      ...omitted,
      emulateMissingAs: 'x86_64',
    }
    const output: T.ImageConfig = {
      source: 'packed',
      arch: ['x86_64'],
      emulateMissing: false,
      nvidiaContainer: false,
    }
    // @ts-expect-error Serialized emulation settings are boolean, not null.
    output.emulateMissing = null
    // @ts-expect-error The legacy architecture must be a string or null.
    const invalid: T.ImageConfigInput = { ...omitted, emulateMissingAs: true }
    expect(nullable.emulateMissing).toBeNull()
    expect(legacy.emulateMissingAs).toBe('x86_64')
    expect(invalid.emulateMissingAs).toBe(true)
  })
})
