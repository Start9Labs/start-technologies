import { z } from 'zod'
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
const literal_0 = /* @__PURE__ */ z.union([z.bigint(), z.number().int().safe()])
const literal_1 = /* @__PURE__ */ z.instanceof(Uint8Array)
const literal_2 = /* @__PURE__ */ z.string()
export const BinaryRequestCommitment: z.ZodObject<{
  timestamp: typeof literal_0
  nonce: typeof literal_0
  size: typeof literal_0
  blake3: typeof literal_1
}> = /* @__PURE__ */ object({
  timestamp: literal_0.meta({
    description: 'A CBOR integer whose JavaScript decoder may return a bigint.',
  }),
  nonce: literal_0.meta({
    description: 'A CBOR integer whose JavaScript decoder may return a bigint.',
  }),
  size: literal_0.meta({
    description: 'A CBOR integer whose JavaScript decoder may return a bigint.',
  }),
  blake3: literal_1.meta({
    description:
      'A binary byte string, without a text encoding or an integer-array fallback.',
  }),
}).meta({
  id: 'BinaryRequestCommitment',
})
export const Guid: typeof literal_2 = /* @__PURE__ */ literal_2.meta({
  id: 'Guid',
})

/**
 * Binary request proofs retain the generic commitment domain, not HTTP header framing.
 */
export const BinaryRequestProof: z.ZodObject<{
  commitment: typeof BinaryRequestCommitment
  signer: typeof literal_1
  signature: typeof literal_1
}> = /* @__PURE__ */ object({
  commitment: BinaryRequestCommitment,
  signer: literal_1.meta({
    description: 'An algorithm-tagged DER value carried as a byte string.',
  }),
  signature: literal_1.meta({
    description: 'An algorithm-tagged DER value carried as a byte string.',
  }),
}).meta({
  id: 'BinaryRequestProof',
  description:
    'Binary request proofs retain the generic commitment domain, not HTTP header framing.',
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
      z.ZodObject<{ ephemeral: typeof literal_1; target: typeof Target }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ role: z.ZodLiteral<'server'> }>,
      z.ZodObject<{
        ephemeral: typeof literal_1
        clientHello: typeof literal_1
      }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ role: z.literal('client') }),
      object({
        ephemeral: literal_1.meta({
          description:
            'A binary byte string, without a text encoding or an integer-array fallback.',
        }),
        target: Target,
      }),
    ),
    z.intersection(
      object({ role: z.literal('server') }),
      object({
        ephemeral: literal_1.meta({
          description:
            'A binary byte string, without a text encoding or an integer-array fallback.',
        }),
        clientHello: literal_1.meta({
          description:
            'A binary byte string, without a text encoding or an integer-array fallback.',
        }),
      }),
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
