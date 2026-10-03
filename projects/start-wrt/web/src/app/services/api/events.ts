export type FlashMode = 'update' | 'fresh-start'
export type FlashModeInput = 'update' | 'fresh-start'
export type FlashParams = {
  mode: FlashMode
  password: string
  timezone: string | null
}
export type FlashParamsInput = {
  mode: FlashModeInput
  password: string
  timezone?: string | null
}
export type FullProgress = { overall: Progress; phases: NamedProgress[] }
export type FullProgressInput = {
  overall: ProgressInput
  phases: NamedProgressInput[]
}
export type NamedProgress = { name: string; progress: Progress }
export type NamedProgressInput = { name: string; progress: ProgressInput }
export type Progress =
  | null
  | boolean
  | { done: number; total: number | null; units: ProgressUnits | null }
export type ProgressInput =
  | null
  | boolean
  | { done: number; total?: number | null; units?: ProgressUnitsInput | null }
export type ProgressUnits = 'bytes' | 'steps'
export type ProgressUnitsInput = 'bytes' | 'steps'
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
