import * as T from '../types'
import * as IST from '../actions/input/inputSpecTypes'
import { Action, ActionInfo } from './setupActions'
import { ExtractInputSpecType } from './input/builder/inputSpec'

/** Receives the action's form as `getInput` opened it and returns the input to submit. */
export type RunActionInput<Input> = (form: {
  spec: IST.InputSpec
  value: T.DeepPartial<Input> | null
}) => Input

/**
 * Runs an action of this service, or of another one whose `access` admits it.
 * An action with input opens its form first, so the input is checked against
 * the form it answers. Calls reuse the caller's event ID when present and
 * must run one input action at a time under that ID. Without a caller event
 * ID, each call uses a fresh ID shared by opening and submitting its form.
 */
export const runAction = async <
  Input extends Record<string, unknown>,
>(options: {
  effects: T.Effects
  packageId?: T.PackageId
  actionId: T.ActionId
  /** Seeds the form, including the values its dynamic fields are computed from. */
  prefill?: T.DeepPartial<Input> | null
  input?: RunActionInput<Input>
}) => {
  const { effects, packageId, actionId } = options
  if (!options.input) return effects.action.run({ packageId, actionId })
  const eventId = effects.eventId ?? crypto.randomUUID()
  const form = await effects.action.getInput(
    actionParams({
      packageId,
      actionId,
      eventId,
      prefill: (options.prefill ?? null) as Record<string, unknown> | null,
    }),
  )
  if (!form) {
    throw new Error(
      `Action ${actionId} of ${packageId ?? 'this service'} has no input form`,
    )
  }
  return effects.action.run(
    actionParams({
      packageId,
      actionId,
      eventId,
      input: options.input({
        spec: form.spec as IST.InputSpec,
        value: form.value as T.DeepPartial<Input> | null,
      }),
    }),
  )
}

function actionParams<Request extends { eventId: string }>(request: Request) {
  // StartOS 0.4.0.2 overwrites action event IDs before JSON serialization.
  // Remove when the SDK minimum reaches StartOS 0.4.0.3.
  return { ...request, toJSON: () => request }
}

type GetActionInputType<A extends ActionInfo<T.ActionId, any>> =
  A extends ActionInfo<T.ActionId, infer I> ? I : never

type TaskBase = {
  reason?: string
  replayId?: string
}
type TaskInput<T extends ActionInfo<T.ActionId, any>> = {
  kind: 'partial'
  accept: T.DeepPartial<GetActionInputType<T>>[]
  set: T.DeepPartial<GetActionInputType<T>>
}
export type TaskOptions<T extends ActionInfo<T.ActionId, any>> = TaskBase &
  (
    | {
        when?: Exclude<T.TaskTrigger, { condition: 'input-not-matches' }>
        input?: TaskInput<T>
      }
    | {
        when: T.TaskTrigger & { condition: 'input-not-matches' }
        input: TaskInput<T>
      }
  )

const _validate: T.Task = {} as TaskOptions<any> & {
  actionId: string
  packageId: string
  severity: T.TaskSeverity
}

/** Recursively converts undefined values to null so they survive JSON serialization */
function undefinedToNull(obj: unknown): unknown {
  if (obj === undefined) return null
  if (obj === null || typeof obj !== 'object') return obj
  if (Array.isArray(obj)) return obj.map(undefinedToNull)
  const result: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(obj)) {
    result[k] = undefinedToNull(v)
  }
  return result
}

export const createTask = <T extends ActionInfo<T.ActionId, any>>(options: {
  effects: T.Effects
  packageId: T.PackageId
  action: T
  severity: T.TaskSeverity
  options?: TaskOptions<T>
}) => {
  const request = options.options || {}
  const actionId = options.action.id
  const input =
    'input' in request && request.input
      ? {
          ...request.input,
          accept: request.input.accept.map(undefinedToNull),
          set: undefinedToNull(request.input.set),
        }
      : (request as any).input
  const req = {
    ...request,
    input,
    actionId,
    packageId: options.packageId,
    action: undefined,
    severity: options.severity,
    replayId: request.replayId || `${options.packageId}:${actionId}`,
  }
  delete req.action
  return options.effects.action.createTask(req)
}
