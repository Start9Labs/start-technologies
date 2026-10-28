import type { z } from 'zod'
import type * as schemas from './events.schemas'
export type FlashMode = z.output<typeof schemas.FlashMode>
export type FlashModeInput = z.input<typeof schemas.FlashMode>
export type ProgressUnits = z.output<typeof schemas.ProgressUnits>
export type ProgressUnitsInput = z.input<typeof schemas.ProgressUnits>

/**
 * Streaming event sent to the frontend during flash.
 */
export type SetupEvent = z.output<typeof schemas.SetupEvent>

/**
 * Streaming event sent to the frontend during flash.
 */
export type SetupEventInput = z.input<typeof schemas.SetupEvent>
export type FlashParams = z.output<typeof schemas.FlashParams>
export type FlashParamsInput = z.input<typeof schemas.FlashParams>

/**
 * Progress state for a single phase or overall operation.
 * Wire-compatible with start-os's `Progress` (serialized as untagged).
 */
export type Progress = z.output<typeof schemas.Progress>

/**
 * Progress state for a single phase or overall operation.
 * Wire-compatible with start-os's `Progress` (serialized as untagged).
 */
export type ProgressInput = z.input<typeof schemas.Progress>
export type NamedProgress = z.output<typeof schemas.NamedProgress>
export type NamedProgressInput = z.input<typeof schemas.NamedProgress>

/**
 * Snapshot of overall + per-phase progress, sent over WebSocket.
 */
export type FullProgress = z.output<typeof schemas.FullProgress>

/**
 * Snapshot of overall + per-phase progress, sent over WebSocket.
 */
export type FullProgressInput = z.input<typeof schemas.FullProgress>
