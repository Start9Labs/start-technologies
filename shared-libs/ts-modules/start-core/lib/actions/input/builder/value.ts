import { InputSpec, LazyBuild } from './inputSpec'
import { List } from './list'
import { UnionRes, UnionResStaticValidatedAs, Variants } from './variants'
import {
  Pattern,
  RandomString,
  ValueSpec,
  ValueSpecDatetime,
  ValueSpecHidden,
  ValueSpecText,
  ValueSpecTextarea,
} from '../inputSpecTypes'
import { DefaultString } from '../inputSpecTypes'
import { _, once } from '../../../util'
import { withPatterns } from './patternCheck'
import { z } from '../../../zExport'
import { DeepPartial } from '../../../types'

/** Build a union-of-literals validator from object keys, falling back to z.string() when empty */
function literalKeysValidator(
  values: Record<string, unknown>,
): z.ZodType<string> {
  const keys = Object.keys(values)
  if (keys.length === 0) return z.string()
  return z.union(
    keys.map(x => z.literal(x)) as [
      z.ZodLiteral<string>,
      z.ZodLiteral<string>,
      ...z.ZodLiteral<string>[],
    ],
  )
}

/** Zod schema for a file upload result — validates `{ path, commitment: { hash, size } }`. */
export const fileInfoParser = z.looseObject({
  path: z.string(),
  commitment: z.looseObject({ hash: z.string(), size: z.number() }),
})
/** The parsed result of a file upload, containing the file path and its content commitment (hash + size). */
export type FileInfo = z.infer<typeof fileInfoParser>

/** Conditional type: returns `T` if `Required` is `true`, otherwise `T | null`. */
export type AsRequired<T, Required extends boolean> = Required extends true
  ? T
  : T | null

const testForAsRequiredParser = once(
  () => (v: unknown) =>
    z.looseObject({ required: z.literal(true) }).safeParse(v).success,
)
function asRequiredParser<Type, Input extends { required: boolean }>(
  parser: z.ZodType<Type>,
  input: Input,
): z.ZodType<AsRequired<Type, Input['required']>> {
  if (testForAsRequiredParser()(input)) return parser as any
  return parser.nullable() as any
}

/**
 * Core builder class for defining a single form field in a service configuration spec.
 *
 * Each static factory method (e.g. `Value.text()`, `Value.toggle()`, `Value.select()`) creates
 * a typed `Value` instance representing a specific field type. Dynamic variants (e.g. `Value.dynamicText()`)
 * allow the field options to be computed lazily at runtime.
 *
 * Use with {@link InputSpec} to compose complete form specifications.
 *
 * @typeParam Type - The runtime type this field produces when filled in
 * @typeParam StaticValidatedAs - The compile-time validated type (usually same as Type)
 * @typeParam OuterType - The parent form's type context (used by dynamic variants)
 */
export class Value<
  Type extends StaticValidatedAs,
  StaticValidatedAs = Type,
  OuterType = unknown,
> {
  protected constructor(
    public build: LazyBuild<
      {
        spec: ValueSpec
        validator: z.ZodType<Type>
      },
      OuterType
    >,
    public readonly validator: z.ZodType<StaticValidatedAs>,
  ) {}
  public _TYPE: Type = null as any as Type
  public _PARTIAL: DeepPartial<Type> = null as any as DeepPartial<Type>
  /** @internal Used by {@link InputSpec.filter} to support nested filtering of object-typed fields. */
  _objectSpec?: {
    inputSpec: InputSpec<any, any>
    params: { name: string; description?: string | null }
  }

  /**
   * @description Displays a boolean toggle to enable/disable
   * @example
   * ```
    toggleExample: Value.toggle({
      // required
      name: 'Toggle Example',
      default: true,
    
      // optional
      description: null,
      warning: null,
      immutable: false,
    }),
    * ```
   */
  static toggle(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    default: boolean
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = z.boolean()
    return new Value<boolean>(
      async () => ({
        spec: {
          description: null,
          warning: null,
          footnote: null,
          type: 'toggle' as const,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.toggle} but options are resolved lazily at runtime via a builder function. */
  static dynamicToggle<OuterType = unknown>(
    a: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: boolean
        disabled?: false | string
      },
      OuterType
    >,
  ) {
    const validator = z.boolean()
    return new Value<boolean, boolean, OuterType>(
      async options => ({
        spec: {
          description: null,
          warning: null,
          footnote: null,
          type: 'toggle' as const,
          disabled: false,
          immutable: false,
          ...(await a(options)),
        },
        validator,
      }),
      validator,
    )
  }
  /**
   * @description Displays a three-state toggle — a boolean toggle with a neutral middle
   * position, rendered as three icon buttons (✕ / — / ✓). The left icon outputs `false`, the
   * right outputs `true`, and the middle outputs `null` (meaning "no opinion, use the default").
   * @example
   * ```
    triStateExample: Value.triState({
      // required
      name: 'Allow Uploads',
      default: null,        // null = middle; or true / false

      // optional
      description: null,
      warning: null,
      footnote: null,
      immutable: false,
    }),
    * ```
   */
  static triState(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * @description Initial selection.
     * - `false` → left (✕)
     * - `true` → right (✓)
     * - `null` → middle (—)
     */
    default: boolean | null
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = z.boolean().nullable()
    return new Value<boolean | null>(
      async () => ({
        spec: {
          type: 'triState' as const,
          description: null,
          warning: null,
          footnote: null,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.triState} but options are resolved lazily at runtime via a builder function. */
  static dynamicTriState<OuterType = unknown>(
    a: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: boolean | null
        disabled?: false | string
      },
      OuterType
    >,
  ) {
    const validator = z.boolean().nullable()
    return new Value<boolean | null, boolean | null, OuterType>(
      async options => ({
        spec: {
          type: 'triState' as const,
          description: null,
          warning: null,
          footnote: null,
          disabled: false,
          immutable: false,
          ...(await a(options)),
        },
        validator,
      }),
      validator,
    )
  }
  /**
   * @description Displays a text input field
   * @example
   * ```
    textExample: Value.text({
      // required
      name: 'Text Example',
      required: false,
      default: null,
    
      // optional
      description: null,
      placeholder: null,
      warning: null,
      generate: null,
      inputmode: 'text',
      masked: false,
      minLength: null,
      maxLength: null,
      patterns: [],
      immutable: false,
    }),
    * ```
    */
  static text<Required extends boolean>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * provide a default value.
     * @type { string | RandomString | null }
     * @example default: null
     * @example default: 'World'
     * @example default: { charset: 'abcdefg', len: 16 }
     */
    default: string | RandomString | null
    required: Required
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
    inputmode?: ValueSpecText['inputmode']
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
    /**
     * @description Displays a button that will generate a random string according to the provided charset and len attributes.
     */
    generate?: RandomString | null
  }) {
    const validator = asRequiredParser(withPatterns(z.string(), a.patterns), a)
    return new Value<AsRequired<string, Required>>(
      async () => ({
        spec: {
          type: 'text' as const,
          description: null,
          warning: null,
          footnote: null,
          masked: false,
          placeholder: null,
          minLength: null,
          maxLength: null,
          patterns: [],
          inputmode: 'text',
          disabled: false,
          immutable: a.immutable ?? false,
          generate: a.generate ?? null,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.text} but options are resolved lazily at runtime via a builder function. */
  static dynamicText<Required extends boolean, OuterType = unknown>(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: DefaultString | null
        required: Required
        masked?: boolean
        placeholder?: string | null
        minLength?: number | null
        maxLength?: number | null
        patterns?: Pattern[]
        inputmode?: ValueSpecText['inputmode']
        disabled?: string | false
        generate?: null | RandomString
      },
      OuterType
    >,
  ) {
    return new Value<AsRequired<string, Required>, string | null, OuterType>(
      async options => {
        const a = await getA(options)
        return {
          spec: {
            type: 'text' as const,
            description: null,
            warning: null,
            footnote: null,
            masked: false,
            placeholder: null,
            minLength: null,
            maxLength: null,
            patterns: [],
            inputmode: 'text',
            disabled: false,
            immutable: false,
            generate: a.generate ?? null,
            ...a,
          },
          validator: asRequiredParser(withPatterns(z.string(), a.patterns), a),
        }
      },
      z.string().nullable(),
    )
  }
  /**
   * @description Displays a large textarea field for long form entry.
   * @example
   * ```
    textareaExample: Value.textarea({
      // required
      name: 'Textarea Example',
      required: false,
      default: null,
    
      // optional
      description: null,
      placeholder: null,
      warning: null,
      minLength: null,
      maxLength: null,
      minRows: 3
      maxRows: 6
      immutable: false,
    }),
    * ```
    */
  static textarea<Required extends boolean>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    default: string | null
    required: Required
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
    /** Defaults to 3 */
    minRows?: number
    /** Maximum number of rows before scroll appears. Defaults to 6 */
    maxRows?: number
    placeholder?: string | null
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = asRequiredParser(withPatterns(z.string(), a.patterns), a)
    return new Value<AsRequired<string, Required>>(async () => {
      const built: ValueSpecTextarea = {
        description: null,
        warning: null,
        footnote: null,
        minLength: null,
        maxLength: null,
        patterns: [],
        minRows: 3,
        maxRows: 6,
        placeholder: null,
        type: 'textarea' as const,
        disabled: false,
        immutable: a.immutable ?? false,
        ...a,
      }
      return { spec: built, validator }
    }, validator)
  }
  /** Like {@link Value.textarea} but options are resolved lazily at runtime via a builder function. */
  static dynamicTextarea<Required extends boolean, OuterType = unknown>(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: string | null
        required: Required
        minLength?: number | null
        maxLength?: number | null
        patterns?: Pattern[]
        minRows?: number
        maxRows?: number
        placeholder?: string | null
        disabled?: false | string
      },
      OuterType
    >,
  ) {
    return new Value<AsRequired<string, Required>, string | null, OuterType>(
      async options => {
        const a = await getA(options)
        return {
          spec: {
            description: null,
            warning: null,
            footnote: null,
            minLength: null,
            maxLength: null,
            patterns: [],
            minRows: 3,
            maxRows: 6,
            placeholder: null,
            type: 'textarea' as const,
            disabled: false,
            immutable: false,
            ...a,
          },
          validator: asRequiredParser(withPatterns(z.string(), a.patterns), a),
        }
      },
      z.string().nullable(),
    )
  }
  /**
   * @description Displays a number input field
   * @example
   * ```
    numberExample: Value.number({
      // required
      name: 'Number Example',
      required: false,
      default: null,
      integer: true,
    
      // optional
      description: null,
      placeholder: null,
      warning: null,
      min: null,
      max: null,
      immutable: false,
      step: null,
      units: null,
    }),
    * ```
    */
  static number<Required extends boolean>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * @description optionally provide a default value.
     * @type { default: number | null }
     * @example default: null
     * @example default: 7
     */
    default: number | null
    required: Required
    min?: number | null
    max?: number | null
    /**
     * @description How much does the number increase/decrease when using the arrows provided by the browser.
     * @default 1
     */
    step?: number | null
    /**
     * @description Requires the number to be an integer.
     */
    integer: boolean
    /**
     * @description Optionally display units to the right of the input box.
     */
    units?: string | null
    placeholder?: string | null
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = asRequiredParser(z.number(), a)
    return new Value<AsRequired<number, Required>>(
      () => ({
        spec: {
          type: 'number' as const,
          description: null,
          warning: null,
          footnote: null,
          min: null,
          max: null,
          step: null,
          units: null,
          placeholder: null,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.number} but options are resolved lazily at runtime via a builder function. */
  static dynamicNumber<Required extends boolean, OuterType = unknown>(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: number | null
        required: Required
        min?: number | null
        max?: number | null
        step?: number | null
        integer: boolean
        units?: string | null
        placeholder?: string | null
        disabled?: false | string
      },
      OuterType
    >,
  ) {
    return new Value<AsRequired<number, Required>, number | null, OuterType>(
      async options => {
        const a = await getA(options)
        return {
          spec: {
            type: 'number' as const,
            description: null,
            warning: null,
            footnote: null,
            min: null,
            max: null,
            step: null,
            units: null,
            placeholder: null,
            disabled: false as const,
            immutable: false,
            ...a,
          },
          validator: asRequiredParser(z.number(), a),
        }
      },
      z.number().nullable(),
    )
  }
  /**
   * @description Displays a color field with a clickable swatch that opens the browser-native color picker, plus a text area for typing a hex value.
   * @example
   * ```
    colorExample: Value.color({
      // required
      name: 'Color Example',
      required: false,
      default: null,
    
      // optional
      description: null,
      warning: null,
      immutable: false,
    }),
    * ```
    */
  static color<Required extends boolean>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * @description optionally provide a default value.
     * @type { default: string | null }
     * @example default: null
     * @example default: 'ffffff'
     */
    default: string | null
    required: Required
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = asRequiredParser(z.string(), a)
    return new Value<AsRequired<string, Required>>(
      () => ({
        spec: {
          type: 'color' as const,
          description: null,
          warning: null,
          footnote: null,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }

  /** Like {@link Value.color} but options are resolved lazily at runtime via a builder function. */
  static dynamicColor<Required extends boolean, OuterType = unknown>(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: string | null
        required: Required
        disabled?: false | string
      },
      OuterType
    >,
  ) {
    return new Value<AsRequired<string, Required>, string | null, OuterType>(
      async options => {
        const a = await getA(options)
        return {
          spec: {
            type: 'color' as const,
            description: null,
            warning: null,
            footnote: null,
            disabled: false,
            immutable: false,
            ...a,
          },
          validator: asRequiredParser(z.string(), a),
        }
      },
      z.string().nullable(),
    )
  }
  /**
   * @description Displays a browser-native date/time selector.
   * @example
   * ```
    datetimeExample: Value.datetime({
      // required
      name: 'Datetime Example',
      required: false,
      default: null,
    
      // optional
      description: null,
      warning: null,
      immutable: false,
      inputmode: 'datetime-local',
      min: null,
      max: null,
    }),
    * ```
    */
  static datetime<Required extends boolean>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * @description optionally provide a default value.
     * @type { default: string | null }
     * @example default: null
     * @example default: '1985-12-16 18:00:00.000'
     */
    default: string | null
    required: Required
    /**
     * @description Informs the browser how to behave and which date/time component to display.
     * @default "datetime-local"
     */
    inputmode?: ValueSpecDatetime['inputmode']
    min?: string | null
    max?: string | null
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = asRequiredParser(z.string(), a)
    return new Value<AsRequired<string, Required>>(
      () => ({
        spec: {
          type: 'datetime' as const,
          description: null,
          warning: null,
          footnote: null,
          inputmode: 'datetime-local',
          min: null,
          max: null,
          step: null,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.datetime} but options are resolved lazily at runtime via a builder function. */
  static dynamicDatetime<Required extends boolean, OuterType = unknown>(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: string | null
        required: Required
        inputmode?: ValueSpecDatetime['inputmode']
        min?: string | null
        max?: string | null
        disabled?: false | string
      },
      OuterType
    >,
  ) {
    return new Value<AsRequired<string, Required>, string | null, OuterType>(
      async options => {
        const a = await getA(options)
        return {
          spec: {
            type: 'datetime' as const,
            description: null,
            warning: null,
            footnote: null,
            inputmode: 'datetime-local',
            min: null,
            max: null,
            disabled: false,
            immutable: false,
            ...a,
          },
          validator: asRequiredParser(z.string(), a),
        }
      },
      z.string().nullable(),
    )
  }
  /**
   * @description Displays a select modal with radio buttons, allowing for a single selection.
   * @example
   * ```
    selectExample: Value.select({
      // required
      name: 'Select Example',
      default: 'radio1',
      values: {
        radio1: 'Radio 1',
        radio2: 'Radio 2',
      },
    
      // optional
      description: null,
      warning: null,
      immutable: false,
      disabled: false,
    }),
    * ```
    */
  static select<Values extends Record<string, string>>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * @description Determines if the field is required. If so, optionally provide a default value from the list of values.
     * @type { (keyof Values & string) | null }
     * @example default: null
     * @example default: 'radio1'
     */
    default: keyof Values & string
    /**
     * @description A mapping of unique radio options to their human readable display format.
     * @example
     * ```
      {
        radio1: "Radio 1"
        radio2: "Radio 2"
        radio3: "Radio 3"
      }
     * ```
     */
    values: Values
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = literalKeysValidator(a.values)
    return new Value<keyof Values & string>(
      () => ({
        spec: {
          description: null,
          warning: null,
          footnote: null,
          type: 'select' as const,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.select} but options are resolved lazily at runtime via a builder function. */
  static dynamicSelect<
    Values extends Record<string, string>,
    OuterType = unknown,
  >(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: string
        values: Values
        disabled?: false | string | string[]
      },
      OuterType
    >,
  ) {
    return new Value<keyof Values & string, keyof Values & string, OuterType>(
      async options => {
        const a = await getA(options)
        return {
          spec: {
            description: null,
            warning: null,
            footnote: null,
            type: 'select' as const,
            disabled: false,
            immutable: false,
            ...a,
          },
          validator: literalKeysValidator(a.values),
        }
      },
      z.string(),
    )
  }
  /**
   * @description Displays a select modal with checkboxes, allowing for multiple selections.
   * @example
   * ```
    multiselectExample: Value.multiselect({
      // required
      name: 'Multiselect Example',
      values: {
        option1: 'Option 1',
        option2: 'Option 2',
      },
      default: [],
    
      // optional
      description: null,
      warning: null,
      immutable: false,
      disabled: false,
      minlength: null,
      maxLength: null,
    }),
    * ```
    */
  static multiselect<Values extends Record<string, string>>(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    /** Supplementary text rendered persistently beneath the field. */
    footnote?: string | null
    /**
     * @description A simple list of which options should be checked by default.
     */
    default: (keyof Values & string)[]
    /**
     * @description A mapping of checkbox options to their human readable display format.
     * @example
     * ```
      {
        option1: "Option 1"
        option2: "Option 2"
        option3: "Option 3"
      }
     * ```
     */
    values: Values
    minLength?: number | null
    maxLength?: number | null
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    const validator = z.array(literalKeysValidator(a.values))
    return new Value<(keyof Values & string)[]>(
      () => ({
        spec: {
          type: 'multiselect' as const,
          minLength: null,
          maxLength: null,
          warning: null,
          description: null,
          footnote: null,
          disabled: false,
          immutable: a.immutable ?? false,
          ...a,
        },
        validator,
      }),
      validator,
    )
  }
  /** Like {@link Value.multiselect} but options are resolved lazily at runtime via a builder function. */
  static dynamicMultiselect<
    Values extends Record<string, string>,
    OuterType = unknown,
  >(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        footnote?: string | null
        default: string[]
        values: Values
        minLength?: number | null
        maxLength?: number | null
        disabled?: false | string | string[]
      },
      OuterType
    >,
  ) {
    return new Value<
      (keyof Values & string)[],
      (keyof Values & string)[],
      OuterType
    >(async options => {
      const a = await getA(options)
      return {
        spec: {
          type: 'multiselect' as const,
          minLength: null,
          maxLength: null,
          warning: null,
          description: null,
          footnote: null,
          disabled: false,
          immutable: false,
          ...a,
        },
        validator: z.array(literalKeysValidator(a.values)),
      }
    }, z.array(z.string()))
  }
  /**
   * @description Display a collapsable grouping of additional fields, a "sub form". The second value is the inputSpec spec for the sub form.
   * @example
   * ```
    objectExample: Value.object(
      {
        // required
        name: 'Object Example',
    
        // optional
        description: null,
        warning: null,
      },
      InputSpec.of({}),
    ),
    * ```
    */
  static object<
    Type extends StaticValidatedAs,
    StaticValidatedAs extends Record<string, any>,
  >(
    a: {
      name: string
      description?: string | null
    },
    spec: InputSpec<Type, StaticValidatedAs>,
  ) {
    const value = new Value<Type, StaticValidatedAs>(async options => {
      const built = await spec.build(options as any)
      return {
        spec: {
          type: 'object' as const,
          description: null,
          warning: null,
          ...a,
          spec: built.spec,
        },
        validator: built.validator,
      }
    }, spec.validator)
    value._objectSpec = { inputSpec: spec, params: a }
    return value
  }
  /**
   * Displays a file upload input field.
   *
   * @param a.extensions - Allowed file extensions (e.g. `[".pem", ".crt"]`)
   * @param a.required - Whether a file must be selected
   */
  static file<Required extends boolean>(a: {
    name: string
    description?: string | null
    warning?: string | null
    extensions: string[]
    required: Required
  }) {
    const buildValue = {
      type: 'file' as const,
      description: null,
      warning: null,
      ...a,
    }
    return new Value<AsRequired<FileInfo, Required>>(
      () => ({
        spec: {
          ...buildValue,
        },
        validator: asRequiredParser(fileInfoParser, a),
      }),
      asRequiredParser(fileInfoParser, a),
    )
  }
  /** Like {@link Value.file} but options are resolved lazily at runtime via a builder function. */
  static dynamicFile<Required extends boolean, OuterType = unknown>(
    a: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        extensions: string[]
        required: Required
      },
      OuterType
    >,
  ) {
    return new Value<
      AsRequired<FileInfo, Required>,
      FileInfo | null,
      OuterType
    >(async options => {
      const spec = {
        type: 'file' as const,
        description: null,
        warning: null,
        ...(await a(options)),
      }
      return {
        spec,
        validator: asRequiredParser(fileInfoParser, spec),
      }
    }, fileInfoParser.nullable())
  }
  /**
   * @description Displays a dropdown, allowing for a single selection. Depending on the selection, a different object ("sub form") is presented.
   * @example
   * ```
    unionExample: Value.union(
      {
        // required
        name: 'Union Example',
        default: 'option1',
    
        // optional
        description: null,
        warning: null,
        disabled: false,
        immutable: false,
      },
      Variants.of({
        option1: {
          name: 'Option 1',
          spec: InputSpec.of({}),
        },
        option2: {
          name: 'Option 2',
          spec: InputSpec.of({}),
        },
      }),
    ),
    * ```
    */
  static union<
    VariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any>
      }
    },
  >(a: {
    name: string
    description?: string | null
    /** Presents a warning prompt before permitting the value to change. */
    warning?: string | null
    variants: Variants<VariantValues>
    /**
     * @description Provide a default value from the list of variants.
     * @type { string }
     * @example default: 'variant1'
     */
    default: keyof VariantValues & string
    /**
     * @description Once set, the value can never be changed.
     * @default false
     */
    immutable?: boolean
  }) {
    return new Value<
      typeof a.variants._TYPE,
      typeof a.variants.validator._output
    >(async options => {
      const built = await a.variants.build(options as any)
      return {
        spec: {
          type: 'union' as const,
          description: null,
          warning: null,
          disabled: false,
          ...a,
          variants: built.spec,
          immutable: a.immutable ?? false,
        },
        validator: built.validator,
      }
    }, a.variants.validator)
  }
  /** Like {@link Value.union} but options (including which variants are available) are resolved lazily at runtime. */
  static dynamicUnion<
    VariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any>
      }
    },
    OuterType = unknown,
  >(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        variants: Variants<VariantValues>
        default: keyof VariantValues & string
        disabled: string[] | false | string
      },
      OuterType
    >,
  ): Value<UnionRes<VariantValues>, UnionRes<VariantValues>, OuterType>
  /** Like {@link Value.union} but options are resolved lazily, with an explicit static validator type. */
  static dynamicUnion<
    StaticVariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any, any>
      }
    },
    VariantValues extends StaticVariantValues,
    OuterType = unknown,
  >(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        variants: Variants<VariantValues>
        default: keyof VariantValues & string
        disabled: string[] | false | string
      },
      OuterType
    >,
    validator: z.ZodType<UnionResStaticValidatedAs<StaticVariantValues>>,
  ): Value<
    UnionRes<VariantValues>,
    UnionResStaticValidatedAs<StaticVariantValues>,
    OuterType
  >
  static dynamicUnion<
    VariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any>
      }
    },
    OuterType = unknown,
  >(
    getA: LazyBuild<
      {
        name: string
        description?: string | null
        warning?: string | null
        variants: Variants<VariantValues>
        default: keyof VariantValues & string
        disabled: string[] | false | string
      },
      OuterType
    >,
    validator: z.ZodType<unknown> = z.any(),
  ) {
    return new Value<
      UnionRes<VariantValues>,
      z.infer<typeof validator>,
      OuterType
    >(async options => {
      const newValues = await getA(options)
      const built = await newValues.variants.build(options as any)
      return {
        spec: {
          type: 'union' as const,
          description: null,
          warning: null,
          ...newValues,
          variants: built.spec,
          immutable: false,
        },
        validator: built.validator,
      }
    }, validator)
  }
  /**
   * @description Presents an interface to add/remove/edit items in a list.
   * @example
   * In this example, we create a list of text inputs.
   * 
   * ```
    listExampleText: Value.list(
      List.text(
        {
          // required
          name: 'Text List',

          // optional
          description: null,
          warning: null,
          default: [],
          minLength: null,
          maxLength: null,
        },
        {
          // required
          patterns: [],

          // optional
          placeholder: null,
          generate: null,
          inputmode: 'url',
          masked: false,
          minLength: null,
          maxLength: null,
        },
      ),
    ),
   * ```
   * @example
   * In this example, we create a list of objects.
   * 
   * ```
    listExampleObject: Value.list(
      List.obj(
        {
          // required
          name: 'Object List',

          // optional
          description: null,
          warning: null,
          default: [],
          minLength: null,
          maxLength: null,
        },
        {
          // required
          spec: InputSpec.of({}),

          // optional
          displayAs: null,
          uniqueBy: null,
        },
      ),
    ),
   * ```
   */
  static list<Type>(a: List<Type>) {
    return new Value<Type>(options => a.build(options), a.validator)
  }

  /**
   * @description Provides a way to define a hidden field with a static value. Useful for tracking 
   * @example
   * ```
    hiddenExample: Value.hidden(),
   * ```
   */
  static hidden<T>(): Value<T>
  static hidden<T>(parser: z.ZodType<T>): Value<T>
  static hidden<T>(parser: z.ZodType<T> = z.any()) {
    return new Value<T, z.infer<typeof parser>>(async () => {
      return {
        spec: {
          type: 'hidden' as const,
        } as ValueSpecHidden,
        validator: parser,
      }
    }, parser)
  }

  /**
   * @description Provides a way to define a hidden field with a static value. Useful for tracking 
   * @example
   * ```
    hiddenExample: Value.hidden(),
   * ```
   */
  static dynamicHidden<T, OuterType = unknown>(
    getParser: LazyBuild<z.ZodType<T>, OuterType>,
  ) {
    return new Value<T, T, OuterType>(async options => {
      const validator = await getParser(options)
      return {
        spec: {
          type: 'hidden' as const,
        } as ValueSpecHidden,
        validator,
      }
    }, z.any())
  }

  /**
   * Returns a new Value that produces the same field spec but with `disabled` set to the given message.
   * The field remains in the form but cannot be edited by the user.
   *
   * @param message - The reason the field is disabled, displayed to the user
   */
  withDisabled(message: string): Value<Type, StaticValidatedAs, OuterType> {
    const original = this
    const v = new Value<Type, StaticValidatedAs, OuterType>(async options => {
      const built = await original.build(options)
      return {
        spec: { ...built.spec, disabled: message } as ValueSpec,
        validator: built.validator,
      }
    }, this.validator)
    v._objectSpec = this._objectSpec
    return v
  }

  /**
   * Transforms the validated output value using a mapping function.
   * The form field itself remains unchanged, but the value is transformed after validation.
   *
   * @param fn - A function to transform the validated value
   */
  map<U>(fn: (value: StaticValidatedAs) => U): Value<U, U, OuterType> {
    return new Value<U, U, OuterType>(async options => {
      const built = await this.build(options)
      return {
        spec: built.spec,
        validator: built.validator.transform(fn),
      }
    }, this.validator.transform(fn))
  }
}
