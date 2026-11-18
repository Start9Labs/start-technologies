import type {
  RpcHandler,
  RpcMethod,
  RpcParamType,
  RpcReturnType,
  RpcEndpointType,
  RpcIsStreaming,
  RpcStreamItemType,
  RpcStreamFinalType,
} from './osBindings/rpc/start-os'

export type RpcCallOptions = {
  timeout?: number
  /** HTTP transport headers; non-HTTP transports may ignore them. */
  headers?: Record<string, string>
}

/** Aborting cancels initialization and iteration, including items queued after a terminal. */
export type RpcStreamOptions = RpcCallOptions & { signal?: AbortSignal }

/** Carries one call to a server; the client never sees a wire format. */
export interface RpcTransport<Options = RpcCallOptions> {
  call(method: string, params: unknown, options?: Options): Promise<unknown>
}

/** Initial success precedes items; the iterator's terminal return value is the final result. */
export type RpcResponseStream<Initial, Item, Final> = {
  initial: Initial
  items: AsyncGenerator<Item, Final, void>
}

/** Iterator disposal cancels this operation, never its siblings or a replacement connection. */
export interface RpcStreamTransport<Options = RpcStreamOptions> {
  stream(
    method: string,
    params: unknown,
    options?: Options,
  ): Promise<RpcResponseStream<unknown, unknown, unknown>>
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

type Children<Tree> = Tree extends { _CHILDREN: infer Nodes }
  ? { [Name in keyof Nodes & string as CamelCase<Name>]: Nodes[Name] }
  : {}

type StreamCallable<Tree, Options> = Tree extends RpcHandler
  ? [RpcEndpointType<Tree, ''>] extends [never]
    ? unknown
    : RpcIsStreaming<Tree, ''> extends false
      ? unknown
      : (
          params: RpcParamType<Tree, ''>,
          options?: Options,
        ) => Promise<
          RpcResponseStream<
            RpcReturnType<Tree, ''>,
            RpcStreamItemType<Tree, ''>,
            RpcStreamFinalType<Tree, ''>
          >
        >
  : unknown

/**
 * A type tree such as `RPC.StartOS` as methods: a callable node is a
 * function, a namespace an object, a callable parent both. Kebab-case names
 * become camelCase; intersect several trees to serve them through one client.
 */
export type RpcClient<Tree, Options = RpcCallOptions> = Callable<
  Tree,
  Options
> & { [Name in keyof Children<Tree>]: RpcClient<Children<Tree>[Name], Options> }

/** Streaming methods of a generated tree, with independent initial, item and terminal types. */
export type RpcStreamClient<Tree, Options = RpcStreamOptions> = StreamCallable<
  Tree,
  Options
> & {
  [Name in keyof Children<Tree>]: RpcStreamClient<Children<Tree>[Name], Options>
}

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

type Invoke<Options> = (
  method: string,
  params: unknown,
  options?: Options,
) => unknown

const node = <Options>(
  invoke: Invoke<Options>,
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
          node(invoke, [...path, rpcMethodSegment(property)]),
        )
      }
      return children.get(property)
    },
    apply: (_, __, [params, options]: [unknown, Options?]) =>
      invoke(path.join('.'), params, options),
  })
}

/**
 * Builds the client without the schema tree: the method is derived from the
 * property path, so a bundle carries no schema a caller does not import.
 */
export const createRpcClient = <Client, Options = RpcCallOptions>(
  transport: RpcTransport<Options>,
): Client => node(transport.call.bind(transport), []) as Client

/** Builds the streaming client with the same method-path proxy as ordinary calls. */
export const createRpcStreamClient = <Client, Options = RpcStreamOptions>(
  transport: RpcStreamTransport<Options>,
): Client => node(transport.stream.bind(transport), []) as Client

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
