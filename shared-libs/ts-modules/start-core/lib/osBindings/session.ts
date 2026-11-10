import type { z } from 'zod'
import type * as schemas from './session.schemas'
export type Envelope = z.output<typeof schemas.Envelope>
export type EnvelopeInput = z.input<typeof schemas.Envelope>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>

/**
 * Logical admission intent, independent of deployment prefixes.
 */
export type Target = z.output<typeof schemas.Target>

/**
 * Logical admission intent, independent of deployment prefixes.
 */
export type TargetInput = z.input<typeof schemas.Target>
export type Hello = z.output<typeof schemas.Hello>
export type HelloInput = z.input<typeof schemas.Hello>
export type Message = z.output<typeof schemas.Message>
export type MessageInput = z.input<typeof schemas.Message>
