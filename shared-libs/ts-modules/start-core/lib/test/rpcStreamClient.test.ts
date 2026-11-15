import type * as RPC from '../osBindings/rpc'
import type * as T from '../osBindings'
import { createRpcStreamClient } from '../rpcClient'
import type {
  RpcStreamClient,
  RpcStreamTransport,
  RpcStreamOptions,
} from '../rpcClient'

type Client = RpcStreamClient<RPC.StartOS>
const initial = { id: 1, value: {} }
const revision = { id: 2, patch: [] }

function transport(items: () => AsyncGenerator<unknown, unknown, void>) {
  const calls: [string, unknown, RpcStreamOptions | undefined][] = []
  const driver: RpcStreamTransport = {
    async stream(method, params, options) {
      calls.push([method, params, options])
      return { initial, items: items() }
    },
  }
  return { calls, client: createRpcStreamClient<Client>(driver) }
}

describe('createRpcStreamClient', () => {
  test('keeps the initiating result, items and terminal return value distinct', async () => {
    const { client, calls } = transport(async function* () {
      yield revision
      return null
    })
    const result = await client.db.subscribe({}, { timeout: 1000 })
    expect(result.initial).toEqual(initial)
    expect(await result.items.next()).toEqual({ done: false, value: revision })
    expect(await result.items.next()).toEqual({ done: true, value: null })
    expect(calls).toEqual([['db.subscribe', {}, { timeout: 1000 }]])
    expect(client.db.subscribe).toBe(client.db.subscribe)
  })

  test('passes initialization cancellation through the generated client unchanged', async () => {
    const { client, calls } = transport(async function* () {
      return null
    })
    const abort = new AbortController()
    const response = await client.db.subscribe({}, { signal: abort.signal })
    expect(calls[0][2]?.signal).toBe(abort.signal)
    await response.items.return(null)
  })

  test('uses generated streaming capability and item/final types', () => {
    const typeChecks = () => {
      const { client } = transport(async function* () {
        return null
      })
      const item: RPC.RpcStreamItemType<RPC.StartOS, 'db.subscribe'> = revision
      const final: RPC.RpcStreamFinalType<RPC.StartOS, 'db.subscribe'> = null
      const streaming: RPC.RpcIsStreaming<RPC.StartOS, 'db.subscribe'> = true
      const ordinary: RPC.RpcIsStreaming<RPC.StartOS, 'echo'> = false
      const updateStreaming: RPC.RpcIsStreaming<RPC.StartOS, 'server.update'> =
        true
      const updateItem: RPC.RpcStreamItemType<RPC.StartOS, 'server.update'> =
        {} as T.FullProgress
      const updateFinal: RPC.RpcStreamFinalType<RPC.StartOS, 'server.update'> =
        null
      const diagnosticItem: RPC.RpcStreamItemType<
        RPC.Diagnostic,
        'diagnostic.update'
      > = {} as T.FullProgress
      const diagnosticFinal: RPC.RpcStreamFinalType<
        RPC.Diagnostic,
        'diagnostic.update'
      > = null
      const sideload = client.package.sideload({})
      const sideloadItem: RPC.RpcStreamItemType<
        RPC.StartOS,
        'package.sideload'
      > = {} as T.FullProgress
      const sideloadFinal: RPC.RpcStreamFinalType<
        RPC.StartOS,
        'package.sideload'
      > = null
      // @ts-expect-error update progress uses the exported FullProgress DTO
      const wrongUpdate: RPC.RpcStreamItemType<RPC.StartOS, 'server.update'> =
        'progress'
      // @ts-expect-error a non-streaming endpoint is not callable on this client
      const call = client.echo({ message: 'not a stream' })
      // @ts-expect-error namespaces remain non-callable
      const namespace = client.db({})
      // @ts-expect-error terminal values cannot be substituted for items
      const wrongItem: RPC.RpcStreamItemType<RPC.StartOS, 'db.subscribe'> =
        final
      // @ts-expect-error the terminal result is null, not a revision
      const wrongFinal: RPC.RpcStreamFinalType<RPC.StartOS, 'db.subscribe'> =
        item
      return [
        streaming,
        ordinary,
        updateStreaming,
        updateItem,
        updateFinal,
        wrongUpdate,
        diagnosticItem,
        diagnosticFinal,
        sideload,
        sideloadItem,
        sideloadFinal,
        call,
        namespace,
        wrongItem,
        wrongFinal,
      ]
    }
    expect(typeof typeChecks).toBe('function')
  })

  test('iterator disposal reaches the transport without starting another operation', async () => {
    let disposed = 0
    const { client, calls } = transport(async function* () {
      try {
        yield revision
        yield { id: 3, patch: [] }
        return null
      } finally {
        disposed++
      }
    })
    const result = await client.db.subscribe({})
    for await (const item of result.items) {
      expect(item.id).toBe(2)
      break
    }
    expect(disposed).toBe(1)
    expect(calls).toHaveLength(1)
  })

  test('terminal errors propagate without replaying the operation', async () => {
    const failure = new Error('source failed')
    const { client, calls } = transport(async function* () {
      yield revision
      throw failure
    })
    const result = await client.db.subscribe({})
    await result.items.next()
    await expect(result.items.next()).rejects.toBe(failure)
    expect(calls).toHaveLength(1)
  })

  test('stream is still an ordinary method name rather than a magic proxy property', async () => {
    type Tree = {
      _PARAMS: {}
      _CHILDREN: {
        stream: { _PARAMS: {}; _RETURN: string; _ITEM: number; _FINAL: boolean }
      }
    }
    const calls: string[] = []
    const client = createRpcStreamClient<RpcStreamClient<Tree>>({
      async stream(method) {
        calls.push(method)
        return {
          initial: 'started',
          items: (async function* () {
            yield 1
            return true
          })(),
        }
      },
    })
    const result = await client.stream({})
    expect(result.initial).toBe('started')
    expect(await result.items.next()).toEqual({ done: false, value: 1 })
    expect(await result.items.next()).toEqual({ done: true, value: true })
    expect(calls).toEqual(['stream'])
  })
})
