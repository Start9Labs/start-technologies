import { InputSpec, LazyBuild } from './inputSpec'
import { AsRequired, FileInfo, Value } from './value'
import { List } from './list'
import { UnionRes, UnionResStaticValidatedAs, Variants } from './variants'
import {
  Pattern,
  RandomString,
  ValueSpecDatetime,
  ValueSpecText,
} from '../inputSpecTypes'
import { DefaultString } from '../inputSpecTypes'
import * as z from '../../../z'
import { ListValueSpecText } from '../inputSpecTypes'

export interface InputSpecTools<OuterType> {
  Value: BoundValue<OuterType>
  Variants: typeof Variants
  InputSpec: typeof InputSpec
  List: BoundList<OuterType>
}

export interface BoundValue<OuterType> {
  // Static (non-dynamic) methods — no OuterType involved
  toggle: typeof Value.toggle
  triState: typeof Value.triState
  text: typeof Value.text
  textarea: typeof Value.textarea
  number: typeof Value.number
  color: typeof Value.color
  datetime: typeof Value.datetime
  select: typeof Value.select
  multiselect: typeof Value.multiselect
  object: typeof Value.object
  file: typeof Value.file
  list: typeof Value.list
  hidden: typeof Value.hidden
  union: typeof Value.union

  // Dynamic methods with OuterType pre-bound (last generic param removed)
  dynamicToggle(
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
  ): Value<boolean, boolean, OuterType>

  dynamicTriState(
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
  ): Value<boolean | null, boolean | null, OuterType>

  dynamicText<Required extends boolean>(
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
  ): Value<AsRequired<string, Required>, string | null, OuterType>

  dynamicTextarea<Required extends boolean>(
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
  ): Value<AsRequired<string, Required>, string | null, OuterType>

  dynamicNumber<Required extends boolean>(
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
  ): Value<AsRequired<number, Required>, number | null, OuterType>

  dynamicColor<Required extends boolean>(
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
  ): Value<AsRequired<string, Required>, string | null, OuterType>

  dynamicDatetime<Required extends boolean>(
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
  ): Value<AsRequired<string, Required>, string | null, OuterType>

  dynamicSelect<Values extends Record<string, string>>(
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
  ): Value<keyof Values & string, keyof Values & string, OuterType>

  dynamicMultiselect<Values extends Record<string, string>>(
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
  ): Value<(keyof Values & string)[], (keyof Values & string)[], OuterType>

  dynamicFile<Required extends boolean>(
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
  ): Value<AsRequired<FileInfo, Required>, FileInfo | null, OuterType>

  dynamicUnion<
    VariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any>
      }
    },
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
  dynamicUnion<
    StaticVariantValues extends {
      [K in string]: {
        name: string
        spec: InputSpec<any, any>
      }
    },
    VariantValues extends StaticVariantValues,
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

  dynamicHidden<T>(
    getParser: LazyBuild<z.ZodType<T>, OuterType>,
  ): Value<T, T, OuterType>
}

export interface BoundList<OuterType> {
  text: typeof List.text
  obj: typeof List.obj
  dynamicText(
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
  ): List<string[], string[], OuterType>
}

export function createInputSpecTools<OuterType>(): InputSpecTools<OuterType> {
  return {
    Value: Value as any as BoundValue<OuterType>,
    Variants,
    InputSpec,
    List: List as any as BoundList<OuterType>,
  }
}
