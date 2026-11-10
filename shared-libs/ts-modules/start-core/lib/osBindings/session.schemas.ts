import { z } from 'zod'
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const literal_0 = /* @__PURE__ */ z.string()
export const Envelope: z.ZodObject<{
  payload: typeof literal_0
  proof: z.ZodString
}> = /* @__PURE__ */ object({ payload: literal_0, proof: z.string() }).meta({
  id: 'Envelope',
})
export const Guid: typeof literal_0 = /* @__PURE__ */ literal_0.meta({
  id: 'Guid',
})

/**
 * Logical admission intent, independent of deployment prefixes.
 */
export const Target: z.ZodUnion<
  readonly [
    z.ZodLiteral<'rpc'>,
    z.ZodObject<{
      /**
       * Redemption of this continuation, not another known capability.
       */
      continuation: typeof Guid
    }>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.literal('rpc').meta({
      description: 'The shared JSON-RPC channel.',
    }),
    object({
      /**
       * Redemption of this continuation, not another known capability.
       */
      continuation: Guid.meta({
        description:
          'Redemption of this continuation, not another known capability.',
      }),
    }).meta({
      description:
        'Redemption of this continuation, not another known capability.',
    }),
  ])
  .meta({
    id: 'Target',
    description:
      'Logical admission intent, independent of deployment prefixes.',
  })
export const Hello: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ role: z.ZodLiteral<'client'> }>,
      z.ZodObject<{ ephemeral: typeof literal_0; target: typeof Target }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ role: z.ZodLiteral<'server'> }>,
      z.ZodObject<{
        ephemeral: typeof literal_0
        clientHello: typeof literal_0
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ role: z.literal('client') }),
      object({ ephemeral: literal_0, target: Target }),
    ),
    z.intersection(
      object({ role: z.literal('server') }),
      object({ ephemeral: literal_0, clientHello: literal_0 }),
    ),
  ])
  .meta({
    id: 'Hello',
  })
export const Message: z.ZodObject<{
  protocol: z.ZodString
  version: z.ZodInt
  hello: typeof Hello
}> = /* @__PURE__ */ object({
  protocol: z.string(),
  version: z.int(),
  hello: Hello,
}).meta({
  id: 'Message',
})
