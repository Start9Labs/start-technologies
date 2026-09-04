import { InputSpec, LazyBuild } from './inputSpec'
import {
  ListValueSpecText,
  Pattern,
  RandomString,
  UniqueBy,
  ValueSpecList,
  ValueSpecListOf,
} from '../inputSpecTypes'
import { withPatterns } from './patternCheck'
import { z } from '../../../zExport'

/**
 * Builder class for defining list-type form fields.
 *
 * A list presents an interface to add, remove, and reorder items. Items can be
 * either text strings ({@link List.text}) or structured objects ({@link List.obj}).
 *
 * Used with {@link Value.list} to include a list field in an {@link InputSpec}.
 */
export class List<
  Type extends StaticValidatedAs,
  StaticValidatedAs = Type,
  OuterType = unknown,
> {
  private constructor(
    public build: LazyBuild<
      {
        spec: ValueSpecList
        validator: z.ZodType<Type>
      },
      OuterType
    >,
    public readonly validator: z.ZodType<StaticValidatedAs>,
  ) {}
  readonly _TYPE: Type = null as any

  /**
   * Creates a list of text input items.
   *
   * @param a - List-level options (name, description, min/max length, defaults)
   * @param aSpec - Item-level options (patterns, input mode, masking, generation)
   */
  static text(
    a: {
      name: string
      description?: string | null
      warning?: string | null
      default?: string[]
      minLength?: number | null
      maxLength?: number | null
    },
    aSpec: {
      /**
       * @description Mask (aka camouflage) text input with dots: ● ● ●
       * @default false
       */
      masked?: boolean
      placeholder?: string | null
      minLength?: number | null
      maxLength?: number | null
      /**
       * @description A list of regular expressions to which the text must conform to pass validation. A human readable description is provided in case the validation fails.
       * @default []
       * @example
       * ```
        [
          {
            regex: "[a-z]",
            description: "May only contain lower case letters from the English alphabet."
          }
        ]
       * ```
       */
      patterns?: Pattern[]
      /**
       * @description Informs the browser how to behave and which keyboard to display on mobile
       * @default "text"
       */
      inputmode?: ListValueSpecText['inputmode']
      /**
       * @description Displays a button that will generate a random string according to the provided charset and len attributes.
       */
      generate?: null | RandomString
    },
  ) {
    const validator = z.array(withPatterns(z.string(), aSpec.patterns))
    return new List<string[]>(() => {
      const spec = {
        type: 'text' as const,
        placeholder: null,
        minLength: null,
        maxLength: null,
        masked: false,
        inputmode: 'text' as const,
        generate: null,
        patterns: aSpec.patterns || [],
        ...aSpec,
      }
      const built: ValueSpecListOf<'text'> = {
        description: null,
        warning: null,
        default: [],
        type: 'list' as const,
        minLength: null,
        maxLength: null,
        disabled: false,
        ...a,
        spec,
      }
      return { spec: built, validator }
    }, validator)
  }

  /** Like {@link List.text} but options are resolved lazily at runtime via a builder function. */
  static dynamicText<OuterType = unknown>(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        default?: string[]
        minLength?: number | null
        maxLength?: number | null
        disabled?: false | string
        generate?: null | RandomString
        spec: {
          masked?: boolean
          placeholder?: string | null
          minLength?: number | null
          maxLength?: number | null
          patterns?: Pattern[]
          inputmode?: ListValueSpecText['inputmode']
        }
      },
      OuterType
    >,
  ) {
    const validator = z.array(z.string())
    return new List<string[], string[], OuterType>(async options => {
      const { spec: aSpec, ...a } = await getA(options)
      const spec = {
        type: 'text' as const,
        placeholder: null,
        minLength: null,
        maxLength: null,
        masked: false,
        inputmode: 'text' as const,
        generate: null,
        patterns: aSpec.patterns || [],
        ...aSpec,
      }
      const built: ValueSpecListOf<'text'> = {
        description: null,
        warning: null,
        default: [],
        type: 'list' as const,
        minLength: null,
        maxLength: null,
        disabled: false,
        ...a,
        spec,
      }

      return {
        spec: built,
        validator: z.array(withPatterns(z.string(), aSpec.patterns)),
      }
    }, validator)
  }

  /**
   * Creates a list of structured object items, each defined by a nested {@link InputSpec}.
   *
   * @param a - List-level options (name, description, min/max length)
   * @param aSpec - Item-level options (the nested spec, display expression, uniqueness constraint)
   */
  static obj<
    Type extends StaticValidatedAs,
    StaticValidatedAs extends Record<string, any>,
  >(
    a: {
      name: string
      description?: string | null
      warning?: string | null
      default?: []
      minLength?: number | null
      maxLength?: number | null
    },
    aSpec: {
      spec: InputSpec<Type, StaticValidatedAs>
      displayAs?: null | string
      uniqueBy?: null | UniqueBy
    },
  ) {
    return new List<Type[], StaticValidatedAs[]>(async options => {
      const { spec: previousSpecSpec, ...restSpec } = aSpec
      const built = await previousSpecSpec.build(options)
      const spec = {
        type: 'object' as const,
        displayAs: null,
        uniqueBy: null,
        ...restSpec,
        spec: built.spec,
      }
      const value = {
        spec,
        default: [],
        ...a,
      }
      return {
        spec: {
          description: null,
          warning: null,
          minLength: null,
          maxLength: null,
          type: 'list' as const,
          disabled: false,
          ...value,
        },
        validator: z.array(built.validator),
      }
    }, z.array(aSpec.spec.validator))
  }
}
