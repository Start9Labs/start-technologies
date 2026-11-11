import type { z } from 'zod'
import type * as schemas from './events.schemas'
export type EncryptedWire = z.output<typeof schemas.EncryptedWire>
export type EncryptedWireInput = z.input<typeof schemas.EncryptedWire>

/**
 * Firmware build identity attached to each RPC reply.
 */
export type FirmwareMetadata = z.output<typeof schemas.FirmwareMetadata>

/**
 * Firmware build identity attached to each RPC reply.
 */
export type FirmwareMetadataInput = z.input<typeof schemas.FirmwareMetadata>
export type FlashMode = z.output<typeof schemas.FlashMode>
export type FlashModeInput = z.input<typeof schemas.FlashMode>
export type ProgressUnits = z.output<typeof schemas.ProgressUnits>
export type ProgressUnitsInput = z.input<typeof schemas.ProgressUnits>

/**
 * Legacy NDJSON adds Complete/Error; typed RPC carries completion in its terminal.
 */
export type SetupEvent = z.output<typeof schemas.SetupEvent>

/**
 * Legacy NDJSON adds Complete/Error; typed RPC carries completion in its terminal.
 */
export type SetupEventInput = z.input<typeof schemas.SetupEvent>
export type PasswordType = z.output<typeof schemas.PasswordType>
export type PasswordTypeInput = z.input<typeof schemas.PasswordType>

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
export type FlashParams = z.output<typeof schemas.FlashParams>
export type FlashParamsInput = z.input<typeof schemas.FlashParams>
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
