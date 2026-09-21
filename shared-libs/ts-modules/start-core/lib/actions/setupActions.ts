import { z } from '../zExport'
import { InitScript } from '../inits'
import * as T from '../types'
import { once } from '../util'
import { InputSpec } from './input/builder'
import { ExtractInputSpecType } from './input/builder/inputSpec'

type MaybeInputSpec<Type> = {} extends Type ? null : InputSpec<Type>
/**
 * The service that reached the action through `effects.action`, which may be
 * this one. `null` for the user, and for StartOS evaluating a task.
 */
export type ActionCaller = T.PackageId | null

export type Run<A extends Record<string, any>> = (options: {
  effects: T.Effects
  input: A
  spec: T.inputSpecTypes.InputSpec
  caller: ActionCaller
}) => Promise<(T.ActionResult & { version: '1' }) | null | void | undefined>
export type GetInput<A extends Record<string, any>> = (options: {
  effects: T.Effects
  prefill: T.DeepPartial<A> | null
  caller: ActionCaller
}) => Promise<null | void | undefined | T.DeepPartial<A>>

export type MaybeFn<T, Opts = { effects: T.Effects }> =
  | T
  | ((options: Opts) => Promise<T>)
function callMaybeFn<T, Opts = { effects: T.Effects }>(
  maybeFn: MaybeFn<T, Opts>,
  options: Opts,
): Promise<T> {
  if (maybeFn instanceof Function) {
    return maybeFn(options)
  } else {
    return Promise.resolve(maybeFn)
  }
}
function mapMaybeFn<T, U>(
  maybeFn: MaybeFn<T>,
  map: (value: T) => U,
): MaybeFn<U> {
  if (maybeFn instanceof Function) {
    return async (...args) => map(await maybeFn(...args))
  } else {
    return map(maybeFn)
  }
}

export interface ActionInfo<
  Id extends T.ActionId,
  Type extends Record<string, any>,
> {
  readonly id: Id
  readonly _INPUT: Type
}

export interface Action<
  Id extends T.ActionId,
  Type extends Record<string, any>,
> extends ActionInfo<Id, Type> {
  exportMetadata(options: { effects: T.Effects }): Promise<T.ActionMetadata>
  getInput(options: {
    effects: T.Effects
    prefill: T.DeepPartial<Type> | null
    caller?: ActionCaller
  }): Promise<T.ActionInput>
  run(options: {
    effects: T.Effects
    input: Type
    caller?: ActionCaller
  }): Promise<T.ActionResult | null>
}

class ActionImpl<
  Id extends T.ActionId,
  Type extends Record<string, any>,
> implements Action<Id, Type> {
  readonly _INPUT: Type = null as any as Type
  private prevInputSpec: Record<
    string,
    { spec: T.inputSpecTypes.InputSpec; validator: z.ZodType<Type> }
  > = {}
  constructor(
    readonly id: Id,
    private readonly metadataFn: MaybeFn<T.ActionMetadata>,
    private readonly inputSpec: MaybeFn<
      MaybeInputSpec<Type>,
      {
        effects: T.Effects
        prefill: unknown | null
        caller: ActionCaller
      }
    >,
    private readonly getInputFn: GetInput<Type>,
    private readonly runFn: Run<Type>,
  ) {}
  async exportMetadata(options: {
    effects: T.Effects
  }): Promise<T.ActionMetadata> {
    const childEffects = options.effects.child(`setupActions/${this.id}`)
    childEffects.constRetry = once(() => {
      this.exportMetadata(options)
    })
    const metadata = await callMaybeFn(this.metadataFn, {
      effects: childEffects,
    })
    await options.effects.action.export({ id: this.id, metadata })
    return metadata
  }
  async getInput({
    caller = null,
    ...rest
  }: {
    effects: T.Effects
    prefill: T.DeepPartial<Type> | null
    caller?: ActionCaller
  }): Promise<T.ActionInput> {
    const options = { ...rest, caller }
    let spec = {}
    if (this.inputSpec) {
      const inputSpec = await callMaybeFn(this.inputSpec, options)
      const built = await inputSpec?.build(options)
      if (built) {
        this.prevInputSpec[options.effects.eventId!] = built
        spec = built.spec
      }
    }
    return {
      eventId: options.effects.eventId!,
      spec,
      value:
        ((await this.getInputFn(options)) as
          | Record<string, unknown>
          | null
          | undefined) || null,
    }
  }
  async run(options: {
    effects: T.Effects
    input: Type
    caller?: ActionCaller
  }): Promise<T.ActionResult | null> {
    let spec = {}
    if (this.inputSpec) {
      const prevInputSpec = this.prevInputSpec[options.effects.eventId!]
      if (!prevInputSpec) {
        throw new Error(
          `getActionInput has not been called for EventID ${options.effects.eventId}`,
        )
      }
      options.input = prevInputSpec.validator.parse(options.input)
      spec = prevInputSpec.spec
    }
    return (
      (await this.runFn({
        effects: options.effects,
        input: options.input,
        spec,
        caller: options.caller ?? null,
      })) ?? null
    )
  }
}

export const Action = {
  withInput<
    Id extends T.ActionId,
    InputSpecType extends InputSpec<Record<string, any>>,
  >(
    id: Id,
    metadata: MaybeFn<Omit<T.ActionMetadata, 'hasInput'>>,
    inputSpec: MaybeFn<
      InputSpecType,
      {
        effects: T.Effects
        prefill: unknown | null
        caller: ActionCaller
      }
    >,
    getInput: GetInput<ExtractInputSpecType<InputSpecType>>,
    run: Run<ExtractInputSpecType<InputSpecType>>,
  ): Action<Id, ExtractInputSpecType<InputSpecType>> {
    return new ActionImpl<Id, ExtractInputSpecType<InputSpecType>>(
      id,
      mapMaybeFn(metadata, m => ({ ...m, hasInput: true })),
      inputSpec as any,
      getInput,
      run,
    )
  },
  withoutInput<Id extends T.ActionId>(
    id: Id,
    metadata: MaybeFn<Omit<T.ActionMetadata, 'hasInput'>>,
    run: Run<{}>,
  ): Action<Id, {}> {
    return new ActionImpl(
      id,
      mapMaybeFn(metadata, m => ({ ...m, hasInput: false })),
      null,
      async () => null,
      run,
    )
  },
}

export interface Actions<
  AllActions extends Record<T.ActionId, Action<T.ActionId, any>>,
> extends InitScript {
  addAction<A extends Action<T.ActionId, any>>(
    action: A,
  ): Actions<AllActions & { [id in A['id']]: A }>
  init(effects: T.Effects): Promise<void>
  get<Id extends T.ActionId>(actionId: Id): AllActions[Id]
}

class ActionsImpl<
  AllActions extends Record<T.ActionId, Action<T.ActionId, any>>,
> implements Actions<AllActions> {
  constructor(private readonly actions: AllActions) {}
  addAction<A extends Action<T.ActionId, any>>(
    action: A,
  ): Actions<AllActions & { [id in A['id']]: A }> {
    return new ActionsImpl({ ...this.actions, [action.id]: action })
  }
  async init(effects: T.Effects): Promise<void> {
    for (let action of Object.values(this.actions)) {
      const fn = async () => {
        let res: (value?: undefined) => void = () => {}
        const complete = new Promise(resolve => {
          res = resolve
        })
        const e: T.Effects = effects.child(action.id)
        e.constRetry = once(() =>
          complete.then(() => fn()).catch(console.error),
        )
        try {
          await action.exportMetadata({ effects: e })
        } finally {
          res()
        }
      }
      await fn()
    }
    await effects.action.clear({ except: Object.keys(this.actions) })
  }
  get<Id extends T.ActionId>(actionId: Id): AllActions[Id] {
    return this.actions[actionId]
  }
}

export const Actions = {
  of(): Actions<{}> {
    return new ActionsImpl({})
  },
}
