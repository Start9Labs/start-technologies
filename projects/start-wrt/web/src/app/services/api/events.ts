export type FlashMode = 'update' | 'fresh-start'
export type FlashModeInput = 'update' | 'fresh-start'
export type FlashParams = {
  mode: FlashMode
  password: string
  /**
   * Browser IANA timezone (e.g. "America/New_York") captured by the wizard.
   * Carried into the fresh eMMC config since the wizard's live set-timezone
   * only reaches the throwaway microSD overlay. Optional for older clients.
   */
  timezone: string | null
}
export type FlashParamsInput = {
  mode: FlashModeInput
  password: string
  /**
   * Browser IANA timezone (e.g. "America/New_York") captured by the wizard.
   * Carried into the fresh eMMC config since the wizard's live set-timezone
   * only reaches the throwaway microSD overlay. Optional for older clients.
   */
  timezone?: string | null
}

/**
 * Snapshot of overall + per-phase progress, sent over WebSocket.
 */
export type FullProgress = { overall: Progress; phases: NamedProgress[] }

/**
 * Snapshot of overall + per-phase progress, sent over WebSocket.
 */
export type FullProgressInput = {
  overall: ProgressInput
  phases: NamedProgressInput[]
}
export type NamedProgress = { name: string; progress: Progress }
export type NamedProgressInput = { name: string; progress: ProgressInput }

/**
 * Progress state for a single phase or overall operation.
 * Wire-compatible with start-os's `Progress` (serialized as untagged).
 */
export type Progress =
  /**
   * Not started yet — serializes as `null`
   */
  | null
  /**
   * Complete — serializes as `true` (success) or `false` (failure)
   */
  | boolean
  /**
   * In progress
   */
  | { done: number; total: number | null; units: ProgressUnits | null }

/**
 * Progress state for a single phase or overall operation.
 * Wire-compatible with start-os's `Progress` (serialized as untagged).
 */
export type ProgressInput =
  /**
   * Not started yet — serializes as `null`
   */
  | null
  /**
   * Complete — serializes as `true` (success) or `false` (failure)
   */
  | boolean
  /**
   * In progress
   */
  | { done: number; total?: number | null; units?: ProgressUnitsInput | null }
export type ProgressUnits = 'bytes' | 'steps'
export type ProgressUnitsInput = 'bytes' | 'steps'

/**
 * Streaming event sent to the frontend during flash.
 */
export type SetupEvent =
  | ({ phase: 'copying' } & {
      copied: number
      total: number
      step: number
      totalSteps: number
    })
  | ({ phase: 'status' } & {
      message: string
      step: number
      totalSteps: number
    })
  | { phase: 'complete' }
  | ({ phase: 'error' } & { message: string })

/**
 * Streaming event sent to the frontend during flash.
 */
export type SetupEventInput =
  | ({ phase: 'copying' } & {
      copied: number
      total: number
      step: number
      totalSteps: number
    })
  | ({ phase: 'status' } & {
      message: string
      step: number
      totalSteps: number
    })
  | { phase: 'complete' }
  | ({ phase: 'error' } & { message: string })
