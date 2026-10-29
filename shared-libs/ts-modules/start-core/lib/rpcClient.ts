import type {
  RpcHandler,
  RpcMethod,
  RpcParamType,
  RpcReturnType,
} from './osBindings/rpc/start-os'

export type RpcCallOptions = {
  timeout?: number
  /** HTTP transport headers; non-HTTP transports may ignore them. */
  headers?: Record<string, string>
}

/** Carries one call to a server; the client never sees a wire format. */
export interface RpcTransport<Options = RpcCallOptions> {
  call(method: string, params: unknown, options?: Options): Promise<unknown>
}

type CamelCase<S extends string> = S extends `${infer Head}-${infer Tail}`
  ? `${Head}${Capitalize<CamelCase<Tail>>}`
  : S

type Callable<Tree, Options> = Tree extends {
  _PARAMS: infer Params
  _RETURN: infer Return
}
  ? (params: Params, options?: Options) => Promise<Return>
  : unknown

type Children<Tree, Options> = Tree extends { _CHILDREN: infer Nodes }
  ? {
      [Name in keyof Nodes & string as CamelCase<Name>]: RpcClient<
        Nodes[Name],
        Options
      >
    }
  : unknown

/**
 * A type tree such as `RPC.StartOS` as methods: a callable node is a
 * function, a namespace an object, a callable parent both. Kebab-case names
 * become camelCase; intersect several trees to serve them through one client.
 */
export type RpcClient<Tree, Options = RpcCallOptions> = Callable<
  Tree,
  Options
> &
  Children<Tree, Options>

/** One handler per method path of a type tree such as `RPC.StartOS`. */
export type RpcHandlers<Tree extends RpcHandler> = {
  [Method in RpcMethod<Tree>]?: (
    params: RpcParamType<Tree, Method>,
  ) => Promise<RpcReturnType<Tree, Method>>
}

/** Inverse of the client's camelCase; a method segment never starts with a digit. */
export const rpcMethodSegment = (property: string) =>
  property.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)

const reserved = new Set([
  ...Object.getOwnPropertyNames(Object.prototype),
  'then',
  'toJSON',
])

const node = <Options>(
  transport: RpcTransport<Options>,
  path: readonly string[],
): unknown => {
  const children = new Map<string, unknown>()
  return new Proxy(() => {}, {
    get: (target, property) => {
      if (typeof property !== 'string' || reserved.has(property))
        return Reflect.get(target, property)
      if (!children.has(property)) {
        children.set(
          property,
          node(transport, [...path, rpcMethodSegment(property)]),
        )
      }
      return children.get(property)
    },
    apply: (_, __, [params, options]: [unknown, Options?]) =>
      transport.call(path.join('.'), params, options),
  })
}

/**
 * Builds the client without the schema tree: the method is derived from the
 * property path, so a bundle carries no schema a caller does not import.
 */
export const createRpcClient = <Client, Options = RpcCallOptions>(
  transport: RpcTransport<Options>,
): Client => node(transport, []) as Client

/** Dispatches to handlers by method path; an unhandled method rejects. */
export const tableTransport = <Tree extends RpcHandler>(
  handlers: RpcHandlers<Tree>,
): RpcTransport<unknown> => ({
  async call(method, params) {
    const handler = (
      handlers as Record<string, (params: unknown) => Promise<unknown>>
    )[method]
    if (!handler) throw new Error(`No handler for ${method}`)
    return handler(params)
  },
})
