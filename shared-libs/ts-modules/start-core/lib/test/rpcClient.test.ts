import type * as RPC from '../osBindings/rpc'
import * as schemas from '../osBindings/schemas'
import {
  createRpcClient,
  rpcMethodSegment,
  tableTransport,
  RpcCallOptions,
  RpcClient,
  RpcTransport,
} from '../rpcClient'

const recording = () => {
  const calls: [string, unknown, RpcCallOptions | undefined][] = []
  const transport: RpcTransport = {
    async call(method, params, options) {
      calls.push([method, params, options])
      return null
    },
  }
  return { calls, transport }
}

type StartOS = RpcClient<RPC.StartOS>

describe('createRpcClient', () => {
  test('derives each dotted method from the property path, camelCase as kebab-case', async () => {
    const { calls, transport } = recording()
    const api = createRpcClient<StartOS>(transport)
    await api.auth.getPubkey({})
    await api.package.logs({ id: 'p', limit: 5 }, { timeout: 10_000 })
    await api.net.gateway.checkPortV6({ gateway: 'g', port: 80 })
    expect(calls).toEqual([
      ['auth.get-pubkey', {}, undefined],
      ['package.logs', { id: 'p', limit: 5 }, { timeout: 10_000 }],
      ['net.gateway.check-port-v6', { gateway: 'g', port: 80 }, undefined],
    ])
  })

  test('a callable parent is a function with its children attached', async () => {
    const { calls, transport } = recording()
    const api = createRpcClient<StartOS>(transport)
    await api.server.logs({ limit: 5 })
    await api.server.logs.follow({})
    expect(calls.map(([method]) => method)).toEqual([
      'server.logs',
      'server.logs.follow',
    ])
    expect(typeof api.server).toBe('function')
  })

  test('types follow the tree, inherited parameters included', () => {
    const typeChecks = () => {
      const api = createRpcClient<StartOS>(recording().transport)
      const remove = api.package.host.address.domain.private.remove
      // @ts-expect-error the parents' `package` and `host` are required
      const rejected = remove({ fqdn: 'a.b' })
      const accepted = remove({ package: 'p', host: 'h', fqdn: 'a.b' })
      // @ts-expect-error a namespace is not callable
      const namespace = api.auth({})
      const result: Promise<Record<string, unknown>> = api.auth.getPubkey({})
      return [rejected, accepted, namespace, result]
    }
    expect(typeof typeChecks).toBe('function')
  })

  test('intersecting trees serves every server state through one client', async () => {
    const { calls, transport } = recording()
    const api = createRpcClient<
      StartOS & RpcClient<RPC.Diagnostic> & RpcClient<RPC.Init>
    >(transport)
    await api.diagnostic.restart({})
    await api.diagnostic.disk.repair({})
    await api.init.subscribe({})
    await api.server.logs({})
    expect(calls.map(([method]) => method)).toEqual([
      'diagnostic.restart',
      'diagnostic.disk.repair',
      'init.subscribe',
      'server.logs',
    ])
  })

  test('a client is neither a thenable nor a fresh object per access', async () => {
    const api = createRpcClient<StartOS>(recording().transport)
    const resolved = await Promise.resolve(api.auth)
    expect(typeof resolved.getPubkey).toBe('function')
    expect(api.auth.getPubkey).toBe(api.auth.getPubkey)
    expect(String(api.auth)).toBeDefined()
  })
})

describe('rpcMethodSegment', () => {
  test('inverts the camelCase of every method path in the generated trees', () => {
    const camelCase = (name: string) =>
      name.replace(/-([a-z0-9])/g, (_, letter: string) => letter.toUpperCase())
    const paths = (node: { _CHILDREN?: Record<string, unknown> }): string[] =>
      Object.entries(node._CHILDREN ?? {}).flatMap(([name, child]) => [
        name,
        ...paths(child as typeof node),
      ])
    for (const tree of Object.values(schemas.RPC)) {
      for (const name of paths(tree)) {
        expect(rpcMethodSegment(camelCase(name))).toBe(name)
      }
    }
  })
})

describe('tableTransport', () => {
  test('routes a method to its handler and rejects an unhandled one by name', async () => {
    const api = createRpcClient<StartOS>(
      tableTransport<RPC.StartOS>({
        echo: async ({ message }) => message.toUpperCase(),
      }),
    )
    await expect(api.echo({ message: 'hi' })).resolves.toBe('HI')
    await expect(api.auth.getPubkey({})).rejects.toThrow(
      'No handler for auth.get-pubkey',
    )
  })
})
