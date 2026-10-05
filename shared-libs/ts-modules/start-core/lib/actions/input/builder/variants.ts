import { DeepPartial } from '../../../types'
import { ValueSpec, ValueSpecUnion } from '../inputSpecTypes'
import {
  LazyBuild,
  InputSpec,
  ExtractInputSpecType,
  ExtractInputSpecStaticValidatedAs,
} from './inputSpec'
import { z } from '../../../zExport'

/**
 * The runtime result type of a discriminated union form field.
 * Contains `selection` (the chosen variant key), `value` (the variant's form data),
 * and optionally `other` (partial data from previously selected variants).
 */
export type UnionRes<
  VariantValues extends {
    [K in string]: {
      name: string
      spec: InputSpec<any>
    }
  },
  K extends keyof VariantValues & string = keyof VariantValues & string,
> = {
  [key in keyof VariantValues]: {
    selection: key
    value: ExtractInputSpecType<VariantValues[key]['spec']>
    other?: {
      [key2 in Exclude<keyof VariantValues & string, key>]?: DeepPartial<
        ExtractInputSpecType<VariantValues[key2]['spec']>
      >
    }
  }
}[K]

/** Like {@link UnionRes} but using the static (Zod-inferred) validated types. */
export type UnionResStaticValidatedAs<
  VariantValues extends {
    [K in string]: {
      name: string
      spec: InputSpec<any>
    }
  },
  K extends keyof VariantValues & string = keyof VariantValues & string,
> = {
  [key in keyof VariantValues]: {
    selection: key
    value: ExtractInputSpecStaticValidatedAs<VariantValues[key]['spec']>
    other?: {
      [key2 in Exclude<keyof VariantValues & string, key>]?: DeepPartial<
        ExtractInputSpecStaticValidatedAs<VariantValues[key2]['spec']>
      >
    }
  }
}[K]

/**
 * Used in the the Value.select { @link './value.ts' }
 * to indicate the type of select variants that are available. The key for the record passed in will be the
 * key to the tag.id in the Value.select
```ts
 
export const disabled = InputSpec.of({});
export const size = Value.number({
  name: "Max Chain Size",
  default: 550,
  description: "Limit of blockchain size on disk.",
  warning: "Increasing this value will require re-syncing your node.",
  required: true,
  range: "[550,1000000)",
  integral: true,
  units: "MiB",
  placeholder: null,
});
export const automatic = InputSpec.of({ size: size });
export const size1 = Value.number({
  name: "Failsafe Chain Size",
  default: 65536,
  description: "Prune blockchain if size expands beyond this.",
  warning: null,
  required: true,
  range: "[550,1000000)",
  integral: true,
  units: "MiB",
  placeholder: null,
});
export const manual = InputSpec.of({ size: size1 });
export const pruningSettingsVariants = Variants.of({
  disabled: { name: "Disabled", spec: disabled },
  automatic: { name: "Automatic", spec: automatic },
  manual: { name: "Manual", spec: manual },
});
export const pruning = Value.union(
  {
    name: "Pruning Settings",
    description:
      '- Disabled: Disable pruning\n- Automatic: Limit blockchain size on disk to a certain number of megabytes\n- Manual: Prune blockchain with the "pruneblockchain" RPC\n',
    warning: null,
    default: "disabled",
  },
  pruningSettingsVariants
);
```
 */
export class Variants<
  VariantValues extends {
    [K in string]: {
      name: string
      spec: InputSpec<any, any>
    }
  },
  OuterType = unknown,
> {
  private constructor(
    public build: LazyBuild<
      {
        spec: ValueSpecUnion['variants']
        validator: z.ZodType<UnionRes<VariantValues>>
      },
      OuterType
    >,
    public readonly validator: z.ZodType<
      UnionResStaticValidatedAs<VariantValues>
    >,
  ) {}
  readonly _TYPE: UnionRes<VariantValues> = null as any
  /**
   * Creates a `Variants` instance from a record mapping variant keys to their display name and form spec.
   *
   * @param a - A record of `{ name: string, spec: InputSpec }` entries, one per variant
   */
  static of<
    VariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any>
      }
    },
  >(a: VariantValues) {
    const staticValidators = {} as {
      [K in keyof VariantValues]: z.ZodType<
        ExtractInputSpecStaticValidatedAs<VariantValues[K]['spec']>
      >
    }
    for (const key in a) {
      const value = a[key]
      staticValidators[key] = value.spec.validator
    }
    const other = z
      .looseObject(
        Object.fromEntries(
          Object.entries(staticValidators).map(([k, v]) => [
            k,
            z.any().optional(),
          ]),
        ),
      )
      .optional()
    return new Variants<VariantValues>(
      async options => {
        const validators = {} as {
          [K in keyof VariantValues]: z.ZodType<
            ExtractInputSpecType<VariantValues[K]['spec']>
          >
        }
        const variants = {} as {
          [K in keyof VariantValues]: {
            name: string
            spec: Record<string, ValueSpec>
          }
        }
        for (const key in a) {
          const value = a[key]
          const built = await value.spec.build(options as any)
          variants[key] = {
            name: value.name,
            spec: built.spec,
          }
          validators[key] = built.validator
        }
        const other = z
          .looseObject(
            Object.fromEntries(
              Object.entries(validators).map(([k, v]) => [
                k,
                z.any().optional(),
              ]),
            ),
          )
          .optional()
        return {
          spec: variants,
          validator: z.union(
            Object.entries(validators).map(([k, v]) =>
              z.looseObject({
                selection: z.literal(k),
                value: v,
                other,
              }),
            ) as [z.ZodObject<any>, z.ZodObject<any>, ...z.ZodObject<any>[]],
          ) as any,
        }
      },
      z.union(
        Object.entries(staticValidators).map(([k, v]) =>
          z.looseObject({
            selection: z.literal(k),
            value: v,
            other,
          }),
        ) as [z.ZodObject<any>, z.ZodObject<any>, ...z.ZodObject<any>[]],
      ) as any,
    )
  }
}
