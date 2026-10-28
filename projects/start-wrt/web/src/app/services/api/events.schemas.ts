import { z } from 'zod'
const wire = <A extends z.ZodType, B extends z.ZodType>(input: A, output: B) =>
  z.codec(input, output, { decode: v => v as never, encode: v => v as never })
const object = <Shape extends z.core.$ZodLooseShape>(shape: Shape) =>
  z.object(shape).meta({ additionalProperties: true })
export const FlashMode: z.ZodEnum<{
  update: 'update'
  'fresh-start': 'fresh-start'
}> = /* @__PURE__ */ z.enum(['update', 'fresh-start']).meta({
  id: 'FlashMode',
})
export const ProgressUnits: z.ZodEnum<{ bytes: 'bytes'; steps: 'steps' }> =
  /* @__PURE__ */ z.enum(['bytes', 'steps']).meta({
    id: 'ProgressUnits',
  })

/**
 * Streaming event sent to the frontend during flash.
 */
export const SetupEvent: z.ZodUnion<
  readonly [
    z.ZodIntersection<
      z.ZodObject<{ phase: z.ZodLiteral<'copying'> }>,
      z.ZodObject<{
        copied: z.ZodInt
        total: z.ZodInt
        step: z.ZodInt
        totalSteps: z.ZodInt
      }>
    >,
    z.ZodIntersection<
      z.ZodObject<{ phase: z.ZodLiteral<'status'> }>,
      z.ZodObject<{
        message: z.ZodString
        step: z.ZodInt
        totalSteps: z.ZodInt
      }>
    >,
    z.ZodObject<{ phase: z.ZodLiteral<'complete'> }>,
    z.ZodIntersection<
      z.ZodObject<{ phase: z.ZodLiteral<'error'> }>,
      z.ZodObject<{ message: z.ZodString }>
    >,
  ]
> = /* @__PURE__ */ z
  .union([
    z.intersection(
      object({ phase: z.literal('copying') }),
      object({
        copied: z.int(),
        total: z.int(),
        step: z.int(),
        totalSteps: z.int(),
      }),
    ),
    z.intersection(
      object({ phase: z.literal('status') }),
      object({ message: z.string(), step: z.int(), totalSteps: z.int() }),
    ),
    object({ phase: z.literal('complete') }),
    z.intersection(
      object({ phase: z.literal('error') }),
      object({ message: z.string() }),
    ),
  ])
  .meta({
    id: 'SetupEvent',
    description: 'Streaming event sent to the frontend during flash.',
  })
export const FlashParams: z.ZodObject<{
  mode: typeof FlashMode
  password: z.ZodString
  /**
   * Browser IANA timezone (e.g. "America/New_York") captured by the wizard.
   * Carried into the fresh eMMC config since the wizard's live set-timezone
   * only reaches the throwaway microSD overlay. Optional for older clients.
   */
  timezone: z.ZodCodec<
    z.ZodOptional<z.ZodNullable<z.ZodString>>,
    z.ZodNullable<z.ZodString>
  >
}> = /* @__PURE__ */ object({
  mode: FlashMode,
  password: z.string(),
  /**
   * Browser IANA timezone (e.g. "America/New_York") captured by the wizard.
   * Carried into the fresh eMMC config since the wizard's live set-timezone
   * only reaches the throwaway microSD overlay. Optional for older clients.
   */
  timezone: wire(z.string().nullable().optional(), z.string().nullable()).meta({
    description:
      'Browser IANA timezone (e.g. "America/New_York") captured by the wizard.\nCarried into the fresh eMMC config since the wizard\'s live set-timezone\nonly reaches the throwaway microSD overlay. Optional for older clients.',
  }),
}).meta({
  id: 'FlashParams',
})

/**
 * Progress state for a single phase or overall operation.
 * Wire-compatible with start-os's `Progress` (serialized as untagged).
 */
export const Progress: z.ZodUnion<
  readonly [
    z.ZodNull,
    z.ZodBoolean,
    z.ZodObject<{
      done: z.ZodInt
      total: z.ZodCodec<
        z.ZodOptional<z.ZodNullable<z.ZodInt>>,
        z.ZodNullable<z.ZodInt>
      >
      units: z.ZodCodec<
        z.ZodOptional<z.ZodNullable<typeof ProgressUnits>>,
        z.ZodNullable<typeof ProgressUnits>
      >
    }>,
  ]
> = /* @__PURE__ */ z
  .union([
    z.null().meta({
      description: 'Not started yet — serializes as `null`',
    }),
    z.boolean().meta({
      description:
        'Complete — serializes as `true` (success) or `false` (failure)',
    }),
    object({
      done: z.int(),
      total: wire(z.int().nullable().optional(), z.int().nullable()),
      units: wire(
        ProgressUnits.nullable().optional(),
        ProgressUnits.nullable(),
      ),
    }).meta({
      description: 'In progress',
    }),
  ])
  .meta({
    id: 'Progress',
    description:
      "Progress state for a single phase or overall operation.\nWire-compatible with start-os's `Progress` (serialized as untagged).",
  })
export const NamedProgress: z.ZodObject<{
  name: z.ZodString
  progress: typeof Progress
}> = /* @__PURE__ */ object({ name: z.string(), progress: Progress }).meta({
  id: 'NamedProgress',
})

/**
 * Snapshot of overall + per-phase progress, sent over WebSocket.
 */
export const FullProgress: z.ZodObject<{
  overall: typeof Progress
  phases: z.ZodArray<typeof NamedProgress>
}> = /* @__PURE__ */ object({
  overall: Progress,
  phases: z.array(NamedProgress),
}).meta({
  id: 'FullProgress',
  description: 'Snapshot of overall + per-phase progress, sent over WebSocket.',
})
