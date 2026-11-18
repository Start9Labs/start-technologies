export type RpcHandler = {
  _PARAMS: unknown
  _RETURN?: unknown
  _ITEM?: unknown
  _FINAL?: unknown
  _CHILDREN?: { [name: string]: RpcHandler }
}

export type RpcMethod<Root extends RpcHandler> =
  | (Root extends { _RETURN: unknown } ? '' : never)
  | (Root extends { _CHILDREN: infer Children }
      ? {
          [Name in keyof Children & string]: Children[Name] extends RpcHandler
            ? RpcMethod<Children[Name]> extends infer ChildMethod extends string
              ? ChildMethod extends ''
                ? Name
                : `${Name}.${ChildMethod}`
              : never
            : never
        }[keyof Children & string]
      : never)

export type RpcEndpointType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: unknown }
    ? Root
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcEndpointType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcEndpointType<Children[Method], ''>
          : never
        : never
    : never

export type RpcEndpointField<
  Endpoint,
  Field extends PropertyKey,
> = Endpoint extends { _RETURN: unknown }
  ? Field extends keyof Endpoint
    ? Endpoint[Field]
    : unknown
  : never

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_PARAMS'>

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_RETURN'>

export type RpcStreamItemType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_ITEM'>

export type RpcStreamFinalType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_FINAL'>

export type RpcIsStreaming<Root extends RpcHandler, Method extends string> = [
  RpcEndpointType<Root, Method>,
] extends [never]
  ? never
  : unknown extends RpcStreamItemType<Root, Method>
    ? boolean
    : [RpcStreamItemType<Root, Method>] extends [never]
      ? false
      : true

export type RpcStreamingMethod<Root extends RpcHandler> = {
  [Method in RpcMethod<Root>]: RpcIsStreaming<Root, Method> extends false
    ? never
    : Method
}[RpcMethod<Root>]

export type RpcSchemaTree<Schema> = {
  [Key in keyof Schema as Key extends '_OPAQUE'
    ? never
    : Key]: Key extends '_PARAMS'
    ? Schema[Key] extends z.ZodType
      ? z.input<Schema[Key]>
      : never
    : Key extends '_RETURN' | '_ITEM' | '_FINAL'
      ? Schema[Key] extends z.ZodType
        ? z.output<Schema[Key]>
        : never
      : Key extends '_CHILDREN'
        ? { [Name in keyof Schema[Key]]: RpcSchemaTree<Schema[Key][Name]> }
        : never
}
