import type { z } from 'zod'
import type * as schemas from './session.schemas'
export type BinaryRequestCommitment = z.output<
  typeof schemas.BinaryRequestCommitment
>
export type BinaryRequestCommitmentInput = z.input<
  typeof schemas.BinaryRequestCommitment
>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>

/**
 * Binary request proofs retain the generic commitment domain, not HTTP header framing.
 */
export type BinaryRequestProof = z.output<typeof schemas.BinaryRequestProof>

/**
 * Binary request proofs retain the generic commitment domain, not HTTP header framing.
 */
export type BinaryRequestProofInput = z.input<typeof schemas.BinaryRequestProof>

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
