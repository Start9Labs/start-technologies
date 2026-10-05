import { ValueSpec } from '../inputSpecTypes'
import { Value } from './value'
import { _ } from '../../../util'
import { Effects } from '../../../Effects'
import { z } from '../../../zExport'
import { zodDeepPartial } from 'zod-deep-partial'
import { DeepPartial } from '../../../types'
import { InputSpecTools, createInputSpecTools } from './inputSpecTools'

/** Options passed to a lazy builder function when resolving dynamic form field values. */
export type LazyBuildOptions<Type> = {
  /** The effects interface for runtime operations (e.g. reading files, querying state). */
  effects: Effects
  /** Previously saved form data to pre-fill the form with, or `null` for fresh creation. */
  prefill: DeepPartial<Type> | null
}
/**
 * A function that lazily produces a value, potentially using effects and prefill data.
 * Used by `dynamic*` variants of {@link Value} to compute form field options at runtime.
 */
export type LazyBuild<ExpectedOut, Type> = (
  options: LazyBuildOptions<Type>,
) => Promise<ExpectedOut> | ExpectedOut

/**
 * Defines which keys to keep when filtering an InputSpec.
 * Use `true` to keep a field as-is, or a nested object to filter sub-fields of an object-typed field.
 */
export type FilterKeys<F> = {
  [K in keyof F]?: F[K] extends Record<string, any>
    ? boolean | FilterKeys<F[K]>
    : boolean
}

type RetainKey<T, F, Default extends boolean> = {
  [K in keyof T]: K extends keyof F
    ? F[K] extends false
      ? never
      : K
    : Default extends true
      ? K
      : never
}[keyof T]

/**
 * Computes the resulting type after applying a {@link FilterKeys} shape to a type.
 */
export type ApplyFilter<T, F, Default extends boolean = false> = {
  [K in RetainKey<T, F, Default>]: K extends keyof F
    ? true extends F[K]
      ? F[K] extends true
        ? T[K]
        : T[K] | undefined
      : T[K] extends Record<string, any>
        ? F[K] extends FilterKeys<T[K]>
          ? ApplyFilter<T[K], F[K]>
          : undefined
        : undefined
    : Default extends true
      ? T[K]
      : undefined
}

/**
 * Computes the union of all valid key-path tuples through a nested type.
 * Each tuple represents a path from root to a field, recursing into object-typed sub-fields.
 */
export type KeyPaths<T> = {
  [K in keyof T & string]: T[K] extends any[]
    ? [K]
    : T[K] extends Record<string, any>
      ? [K] | [K, ...KeyPaths<T[K]>]
      : [K]
}[keyof T & string]

/** Extracts the runtime type from an {@link InputSpec}. */
// prettier-ignore
export type ExtractInputSpecType<A extends InputSpec<Record<string, any>, any>> =
  A extends InputSpec<infer B, any> ? B :
  never

/** Extracts the static validation type from an {@link InputSpec}. */
export type ExtractInputSpecStaticValidatedAs<
  A extends InputSpec<any, Record<string, any>>,
> = A extends InputSpec<any, infer B> ? B : never

// export type ExtractPartialInputSpecType<
//   A extends Record<string, any> | InputSpec<Record<string, any>>,
// > = A extends InputSpec<infer B> ? DeepPartial<B> : DeepPartial<A>

/** Maps an object type to a record of {@link Value} entries for use with `InputSpec.of`. */
export type InputSpecOf<A extends Record<string, any>> = {
  [K in keyof A]: Value<A[K]>
}

/** A value that is either directly provided or lazily computed via a {@link LazyBuild} function. */
export type MaybeLazyValues<A, T> = LazyBuild<A, T> | A
/**
 * InputSpecs are the specs that are used by the os input specification form for this service.
 * Here is an example of a simple input specification
  ```ts
    const smallInputSpec = InputSpec.of({
      test: Value.boolean({
        name: "Test",
        description: "This is the description for the test",
        warning: null,
        default: false,
      }),
    });
  ```

  The idea of an inputSpec is that now the form is going to ask for
  Test: [ ] and the value is going to be checked as a boolean.
  There are more complex values like selects, lists, and objects. See {@link Value}

  Also, there is the ability to get a validator/parser from this inputSpec spec.
  ```ts
  const matchSmallInputSpec = smallInputSpec.validator();
  type SmallInputSpec = typeof matchSmallInputSpec._TYPE;
  ```

  Here is an example of a more complex input specification which came from an input specification for a service
  that works with bitcoin, like c-lightning.
  ```ts

    export const hostname = Value.string({
  name: "Hostname",
  default: null,
  description: "Domain or IP address of bitcoin peer",
  warning: null,
  required: true,
  masked: false,
  placeholder: null,
  pattern:
    "(^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$)|((^(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))$)|(^[a-z2-7]{16}\\.onion$)|(^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\\.)+[a-z0-9][a-z0-9-]{0,61}[a-z0-9]$))",
  patternDescription:
    "Must be either a domain name, or an IPv4 or IPv6 address. Do not include protocol scheme (eg 'http://') or port.",
});
export const port = Value.number({
  name: "Port",
  default: null,
  description: "Port that peer is listening on for inbound p2p connections",
  warning: null,
  required: false,
  range: "[0,65535]",
  integral: true,
  units: null,
  placeholder: null,
});
export const addNodesSpec = InputSpec.of({ hostname: hostname, port: port });

  ```
 */
export class InputSpec<
  Type extends StaticValidatedAs,
  StaticValidatedAs extends Record<string, any> = Type,
> {
  private constructor(
    private readonly spec: {
      [K in keyof Type]: Value<Type[K]>
    },
    public readonly validator: z.ZodType<StaticValidatedAs>,
  ) {}
  public _TYPE: Type = null as any as Type
  public _PARTIAL: DeepPartial<Type> = null as any as DeepPartial<Type>
  public readonly partialValidator: z.ZodType<DeepPartial<StaticValidatedAs>> =
    zodDeepPartial(this.validator) as any
  /**
   * Builds the runtime form specification and combined Zod validator from this InputSpec's fields.
   *
   * @returns An object containing the resolved `spec` (field specs keyed by name) and a combined `validator`
   */
  async build<OuterType>(options: LazyBuildOptions<OuterType>): Promise<{
    spec: {
      [K in keyof Type]: ValueSpec
    }
    validator: z.ZodType<Type>
  }> {
    const answer = {} as {
      [K in keyof Type]: ValueSpec
    }
    const validator = {} as {
      [K in keyof Type]: z.ZodType<any>
    }
    for (const k in this.spec) {
      const built = await this.spec[k].build(options as any)
      answer[k] = built.spec
      validator[k] = built.validator
    }
    return {
      spec: answer,
      validator: z.looseObject(validator) as any,
    }
  }

  /**
   * Adds multiple fields to this spec at once, returning a new `InputSpec` with extended types.
   *
   * @param build - A record of {@link Value} entries, or a function receiving typed tools that returns one
   */
  add<AddSpec extends Record<string, Value<any, any, any>>>(
    build: AddSpec | ((tools: InputSpecTools<Type>) => AddSpec),
  ): InputSpec<
    Type & {
      [K in keyof AddSpec]: AddSpec[K] extends Value<infer T, any, any>
        ? T
        : never
    },
    StaticValidatedAs & {
      [K in keyof AddSpec]: AddSpec[K] extends Value<any, infer S, any>
        ? S
        : never
    }
  > {
    const addedValues =
      build instanceof Function ? build(createInputSpecTools<Type>()) : build
    const newSpec = { ...this.spec, ...addedValues } as any
    const newValidator = z.looseObject(
      Object.fromEntries(
        Object.entries(newSpec).map(([k, v]) => [
          k,
          (v as Value<any>).validator,
        ]),
      ),
    )
    return new InputSpec(newSpec, newValidator as any)
  }

  /**
   * Returns a new InputSpec containing only the specified keys.
   * Use `true` to keep a field as-is, or a nested object to filter sub-fields of object-typed fields.
   *
   * @example
   * ```ts
   * const full = InputSpec.of({
   *   name: Value.text({ name: 'Name', required: true, default: null }),
   *   settings: Value.object({ name: 'Settings' }, InputSpec.of({
   *     debug: Value.toggle({ name: 'Debug', default: false }),
   *     port: Value.number({ name: 'Port', required: true, default: 8080, integer: true }),
   *   })),
   * })
   * const filtered = full.filter({ name: true, settings: { debug: true } })
   * ```
   */
  filter<F extends FilterKeys<Type>, Default extends boolean = false>(
    keys: F,
    keepByDefault?: Default,
  ): InputSpec<
    ApplyFilter<Type, F, Default> & ApplyFilter<StaticValidatedAs, F, Default>,
    ApplyFilter<StaticValidatedAs, F, Default>
  > {
    const newSpec: Record<string, Value<any>> = {}
    for (const k of Object.keys(this.spec)) {
      const filterVal = (keys as any)[k]
      const value = (this.spec as any)[k] as Value<any> | undefined
      if (!value) continue
      if (filterVal === true) {
        newSpec[k] = value
      } else if (typeof filterVal === 'object' && filterVal !== null) {
        const objectMeta = value._objectSpec
        if (objectMeta) {
          const filteredInner = objectMeta.inputSpec.filter(
            filterVal,
            keepByDefault,
          )
          newSpec[k] = Value.object(objectMeta.params, filteredInner)
        } else {
          newSpec[k] = value
        }
      } else if (keepByDefault && filterVal !== false) {
        newSpec[k] = value
      }
    }
    const newValidator = z.looseObject(
      Object.fromEntries(
        Object.entries(newSpec).map(([k, v]) => [k, v.validator]),
      ),
    )
    return new InputSpec(newSpec as any, newValidator as any) as any
  }

  /**
   * Returns a new InputSpec with the specified keys disabled.
   * Use `true` to disable a field, or a nested object to disable sub-fields of object-typed fields.
   * All fields remain in the spec — disabled fields simply cannot be edited by the user.
   *
   * @param keys - Which fields to disable, using the same shape as {@link FilterKeys}
   * @param message - The reason the fields are disabled, displayed to the user
   *
   * @example
   * ```ts
   * const spec = InputSpec.of({
   *   name: Value.text({ name: 'Name', required: true, default: null }),
   *   settings: Value.object({ name: 'Settings' }, InputSpec.of({
   *     debug: Value.toggle({ name: 'Debug', default: false }),
   *     port: Value.number({ name: 'Port', required: true, default: 8080, integer: true }),
   *   })),
   * })
   * const disabled = spec.disable({ name: true, settings: { debug: true } }, 'Managed by the system')
   * ```
   */
  disable(
    keys: FilterKeys<Type>,
    message: string,
  ): InputSpec<Type, StaticValidatedAs> {
    const newSpec: Record<string, Value<any>> = {}
    for (const k in this.spec) {
      const filterVal = (keys as any)[k]
      const value = (this.spec as any)[k] as Value<any>
      if (!filterVal) {
        newSpec[k] = value
      } else if (filterVal === true) {
        newSpec[k] = value.withDisabled(message)
      } else if (typeof filterVal === 'object' && filterVal !== null) {
        const objectMeta = value._objectSpec
        if (objectMeta) {
          const disabledInner = objectMeta.inputSpec.disable(filterVal, message)
          newSpec[k] = Value.object(objectMeta.params, disabledInner)
        } else {
          newSpec[k] = value.withDisabled(message)
        }
      }
    }
    const newValidator = z.looseObject(
      Object.fromEntries(
        Object.entries(newSpec).map(([k, v]) => [k, v.validator]),
      ),
    )
    return new InputSpec(newSpec as any, newValidator as any) as any
  }

  /**
   * Resolves a key path to its corresponding display name path.
   * Each key is mapped to the `name` property of its built {@link ValueSpec}.
   * Recurses into `Value.object` sub-specs for nested paths.
   *
   * @param path - Typed tuple of field keys (e.g. `["settings", "debug"]`)
   * @param options - Build options providing effects and prefill data
   * @returns Array of display names (e.g. `["Settings", "Debug"]`)
   */
  async namePath<OuterType>(
    path: KeyPaths<Type>,
    options: LazyBuildOptions<OuterType>,
  ): Promise<string[]> {
    if (path.length === 0) return []
    const [key, ...rest] = path as [string, ...string[]]
    const value = (this.spec as any)[key] as Value<any> | undefined
    if (!value) return []
    const built = await value.build(options as any)
    const name =
      'name' in built.spec ? (built.spec as { name: string }).name : key
    if (rest.length === 0) return [name]
    const objectMeta = value._objectSpec
    if (objectMeta) {
      const innerNames = await objectMeta.inputSpec.namePath(
        rest as any,
        options,
      )
      return [name, ...innerNames]
    }
    return [name]
  }

  /**
   * Resolves a key path to the description of the target field.
   * Recurses into `Value.object` sub-specs for nested paths.
   *
   * @param path - Typed tuple of field keys (e.g. `["settings", "debug"]`)
   * @param options - Build options providing effects and prefill data
   * @returns The description string, or `null` if the field has no description or was not found
   */
  async description<OuterType>(
    path: KeyPaths<Type>,
    options: LazyBuildOptions<OuterType>,
  ): Promise<string | null> {
    if (path.length === 0) return null
    const [key, ...rest] = path as [string, ...string[]]
    const value = (this.spec as any)[key] as Value<any> | undefined
    if (!value) return null
    if (rest.length === 0) {
      const built = await value.build(options as any)
      return 'description' in built.spec
        ? (built.spec as { description: string | null }).description
        : null
    }
    const objectMeta = value._objectSpec
    if (objectMeta) {
      return objectMeta.inputSpec.description(rest as any, options)
    }
    return null
  }

  /**
   * Returns a new InputSpec filtered to only include keys present in the given partial object.
   * For nested `Value.object` fields, recurses into the partial value to filter sub-fields.
   *
   * @param partial - A deep-partial object whose defined keys determine which fields to keep
   */
  filterFromPartial(
    partial: DeepPartial<Type>,
  ): InputSpec<
    DeepPartial<Type> & DeepPartial<StaticValidatedAs>,
    DeepPartial<StaticValidatedAs>
  > {
    const newSpec: Record<string, Value<any>> = {}
    for (const k of Object.keys(partial)) {
      const value = (this.spec as any)[k] as Value<any> | undefined
      if (!value) continue
      const objectMeta = value._objectSpec
      if (objectMeta) {
        const partialVal = (partial as any)[k]
        if (typeof partialVal === 'object' && partialVal !== null) {
          const filteredInner =
            objectMeta.inputSpec.filterFromPartial(partialVal)
          newSpec[k] = Value.object(objectMeta.params, filteredInner)
          continue
        }
      }
      newSpec[k] = value
    }
    const newValidator = z.looseObject(
      Object.fromEntries(
        Object.entries(newSpec).map(([k, v]) => [k, v.validator]),
      ),
    )
    return new InputSpec(newSpec as any, newValidator as any) as any
  }

  /**
   * Returns a new InputSpec with fields disabled based on which keys are present in the given partial object.
   * For nested `Value.object` fields, recurses into the partial value to disable sub-fields.
   * All fields remain in the spec — disabled fields simply cannot be edited by the user.
   *
   * @param partial - A deep-partial object whose defined keys determine which fields to disable
   * @param message - The reason the fields are disabled, displayed to the user
   */
  disableFromPartial(
    partial: DeepPartial<Type>,
    message: string,
  ): InputSpec<Type, StaticValidatedAs> {
    const newSpec: Record<string, Value<any>> = {}
    for (const k in this.spec) {
      const value = (this.spec as any)[k] as Value<any>
      if (!(k in (partial as any))) {
        newSpec[k] = value
        continue
      }
      const objectMeta = value._objectSpec
      if (objectMeta) {
        const partialVal = (partial as any)[k]
        if (typeof partialVal === 'object' && partialVal !== null) {
          const disabledInner = objectMeta.inputSpec.disableFromPartial(
            partialVal,
            message,
          )
          newSpec[k] = Value.object(objectMeta.params, disabledInner)
          continue
        }
      }
      newSpec[k] = value.withDisabled(message)
    }
    const newValidator = z.looseObject(
      Object.fromEntries(
        Object.entries(newSpec).map(([k, v]) => [k, v.validator]),
      ),
    )
    return new InputSpec(newSpec as any, newValidator as any) as any
  }

  /**
   * Creates an `InputSpec` from a plain record of {@link Value} entries.
   *
   * @example
   * ```ts
   * const spec = InputSpec.of({
   *   username: Value.text({ name: 'Username', required: true, default: null }),
   *   verbose: Value.toggle({ name: 'Verbose Logging', default: false }),
   * })
   * ```
   */
  static of<Spec extends Record<string, Value<any, any>>>(spec: Spec) {
    const validator = z.looseObject(
      Object.fromEntries(
        Object.entries(spec).map(([k, v]) => [k, v.validator]),
      ),
    )
    return new InputSpec<
      {
        [K in keyof Spec]: Spec[K] extends Value<infer T, any, unknown>
          ? T
          : never
      },
      {
        [K in keyof Spec]: Spec[K] extends Value<any, infer T, unknown>
          ? T
          : never
      }
    >(spec, validator as any)
  }
}
