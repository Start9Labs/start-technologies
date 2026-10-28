import { RPC } from '../index'
import { Effects } from '../Effects'

describe('generated effects bindings', () => {
  test('SDK inputs satisfy the host request contracts', () => {
    const task: RPC.RpcParamType<RPC.Effects, 'action.create-task'> = {
      replayId: 'test',
      packageId: 'test',
      actionId: 'configure',
      input: { kind: 'partial', value: { legacy: true } },
    }
    const metadata: RPC.RpcParamType<RPC.Effects, 'action.export'> = {
      id: 'configure',
      metadata: {
        name: 'Configure',
        description: '',
        allowedStatuses: 'any',
        hasInput: false,
      },
    }
    const bind: RPC.RpcParamType<RPC.Effects, 'bind'> = {} as Parameters<
      Effects['bind']
    >[0]
    const input: RPC.RpcParamType<RPC.Effects, 'action.get-input'> =
      {} as Parameters<Effects['action']['getInput']>[0]
    const callback: RPC.RpcParamType<RPC.Effects, 'get-host-info'> = {
      hostId: 'main',
      callback: 1,
    }
    // @ts-expect-error A JS callback must be registered before RPC serialization.
    callback.callback = () => {}
    // @ts-expect-error A namespace is not an invocable method.
    const method: RPC.RpcMethod<RPC.Effects> = 'action'
    // @ts-expect-error The host returns JSON null for a unit result.
    const result: RPC.RpcReturnType<RPC.Effects, 'bind'> = undefined
    expect(task.input).toEqual({ kind: 'partial', value: { legacy: true } })
    expect(metadata.metadata.hasInput).toBe(false)
    expect([bind, input, method, result]).toEqual([{}, {}, 'action', undefined])
  })
})
