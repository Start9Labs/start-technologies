export type RpcHandler = ParentHandler | LeafHandler

export type ParentHandler = {
  _CHILDREN: {
    [name: string]: RpcHandler
  }
  _PARAMS: unknown
  _RETURN?: unknown
}

export type LeafHandler = {
  _PARAMS: unknown
  _RETURN: unknown
}

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = Root['_PARAMS'] &
  (Root extends ParentHandler
    ? Method extends `${infer A}.${infer B}`
      ? RpcParamType<Root['_CHILDREN'][A], B>
      : Root['_CHILDREN'] extends {
            [m in Method]: LeafHandler
          }
        ? Root['_CHILDREN'][Method]['_PARAMS']
        : never
    : never)

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = Root extends ParentHandler
  ? Method extends `${infer A}.${infer B}`
    ? RpcReturnType<Root['_CHILDREN'][A], B>
    : Root['_CHILDREN'] extends {
          [m in Method]: LeafHandler
        }
      ? Root['_CHILDREN'][Method]['_RETURN']
      : never
  : never
