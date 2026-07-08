import { IST, z } from '@start9labs/start-sdk'

export function transformConfigSpec(oldSpec: OldConfigSpec): IST.InputSpec {
  return Object.entries(oldSpec).reduce((inputSpec, [key, oldVal]) => {
    let newVal: IST.ValueSpec

    if (oldVal.type === 'boolean') {
      newVal = {
        type: 'toggle',
        name: oldVal.name,
        default: oldVal.default,
        description: oldVal.description || null,
        warning: oldVal.warning || null,
        footnote: null,
        disabled: false,
        immutable: false,
      }
    } else if (oldVal.type === 'enum') {
      newVal = {
        type: 'select',
        name: oldVal.name,
        description: oldVal.description || null,
        warning: oldVal.warning || null,
        footnote: null,
        default: oldVal.default,
        values: oldVal.values.reduce(
          (obj, curr) => ({
            ...obj,
            [curr]: oldVal['value-names'][curr] || curr,
          }),
          {},
        ),
        disabled: false,
        immutable: false,
      }
    } else if (oldVal.type === 'list') {
      if (isUnionList(oldVal)) return inputSpec
      newVal = getListSpec(oldVal)
    } else if (oldVal.type === 'number') {
      const range = Range.from(oldVal.range)

      newVal = {
        type: 'number',
        name: oldVal.name,
        default: oldVal.default || null,
        description: oldVal.description || null,
        warning: oldVal.warning || null,
        footnote: null,
        disabled: false,
        immutable: false,
        required: !oldVal.nullable,
        min: range.min
          ? range.minInclusive
            ? range.min
            : range.min + 1
          : null,
        max: range.max
          ? range.maxInclusive
            ? range.max
            : range.max - 1
          : null,
        integer: oldVal.integral,
        step: null,
        units: oldVal.units || null,
        placeholder: oldVal.placeholder ? String(oldVal.placeholder) : null,
      }
    } else if (oldVal.type === 'object') {
      newVal = {
        type: 'object',
        name: oldVal.name,
        description: oldVal.description || null,
        warning: oldVal.warning || null,
        spec: transformConfigSpec(matchOldConfigSpec.parse(oldVal.spec)),
      }
    } else if (oldVal.type === 'string') {
      newVal = {
        type: 'text',
        name: oldVal.name,
        default: oldVal.default || null,
        description: oldVal.description || null,
        warning: oldVal.warning || null,
        footnote: null,
        disabled: false,
        immutable: false,
        required: !oldVal.nullable,
        patterns:
          oldVal.pattern && oldVal['pattern-description']
            ? [
                {
                  regex: oldVal.pattern,
                  description: oldVal['pattern-description'],
                },
              ]
            : [],
        minLength: null,
        maxLength: null,
        masked: oldVal.masked || false,
        generate: null,
        inputmode: 'text',
        placeholder: oldVal.placeholder || null,
      }
    } else if (oldVal.type === 'union') {
      newVal = {
        type: 'union',
        name: oldVal.tag.name,
        description: oldVal.tag.description || null,
        warning: oldVal.tag.warning || null,
        variants: Object.entries(oldVal.variants).reduce(
          (obj, [id, spec]) => ({
            ...obj,
            [id]: {
              name: oldVal.tag['variant-names'][id] || id,
              spec: transformConfigSpec(matchOldConfigSpec.parse(spec)),
            },
          }),
          {} as Record<string, { name: string; spec: IST.InputSpec }>,
        ),
        disabled: false,
        default: oldVal.default,
        immutable: false,
      }
    } else if (oldVal.type === 'pointer') {
      return inputSpec
    } else {
      throw new Error(`unknown spec ${JSON.stringify(oldVal)}`)
    }

    return {
      ...inputSpec,
      [key]: newVal,
    }
  }, {} as IST.InputSpec)
}

export function transformOldConfigToNew(
  spec: OldConfigSpec,
  config: Record<string, any>,
): Record<string, any> {
  if (!config) return config
  return Object.entries(spec).reduce((obj, [key, val]) => {
    let newVal = config[key]

    if (isObject(val)) {
      newVal = transformOldConfigToNew(
        matchOldConfigSpec.parse(val.spec),
        config[key],
      )
    }

    if (isUnion(val)) {
      if (!config[key]) return obj

      const selection = config[key]?.[val.tag.id]

      if (!selection) return obj

      delete config[key][val.tag.id]

      if (!val.variants[selection]) return obj

      newVal = {
        selection,
        value: transformOldConfigToNew(
          matchOldConfigSpec.parse(val.variants[selection]),
          config[key],
        ),
      }
    }

    if (isList(val)) {
      if (!config[key]) return obj

      if (isObjectList(val)) {
        newVal = (config[key] as object[]).map(obj =>
          transformOldConfigToNew(matchOldConfigSpec.parse(val.spec.spec), obj),
        )
      } else if (isUnionList(val)) return obj
    }

    if (isPointer(val)) {
      return obj
    }

    return {
      ...obj,
      [key]: newVal,
    }
  }, {})
}

export function transformNewConfigToOld(
  spec: OldConfigSpec,
  config: Record<string, any>,
): Record<string, any> {
  if (!config) return config
  return Object.entries(spec).reduce((obj, [key, val]) => {
    let newVal = config[key]

    if (isObject(val)) {
      newVal = transformNewConfigToOld(
        matchOldConfigSpec.parse(val.spec),
        config[key],
      )
    }

    if (isUnion(val)) {
      newVal = {
        [val.tag.id]: config[key].selection,
        ...transformNewConfigToOld(
          matchOldConfigSpec.parse(val.variants[config[key].selection]),
          config[key].value,
        ),
      }
    }

    if (isList(val)) {
      if (isObjectList(val)) {
        newVal = (config[key] as object[]).map(obj =>
          transformNewConfigToOld(matchOldConfigSpec.parse(val.spec.spec), obj),
        )
      } else if (isUnionList(val)) return obj
    }

    return {
      ...obj,
      [key]: newVal,
    }
  }, {})
}

function getListSpec(
  oldVal: OldValueSpecList,
): IST.ValueSpecMultiselect | IST.ValueSpecList {
  const range = Range.from(oldVal.range)

  let partial: Omit<IST.ValueSpecList, 'type' | 'spec' | 'default'> = {
    name: oldVal.name,
    description: oldVal.description || null,
    warning: oldVal.warning || null,
    minLength: range.min
      ? range.minInclusive
        ? range.min
        : range.min + 1
      : null,
    maxLength: range.max
      ? range.maxInclusive
        ? range.max
        : range.max - 1
      : null,
    disabled: false,
  }

  if (isEnumList(oldVal)) {
    return {
      ...partial,
      type: 'multiselect',
      default: oldVal.default as string[],
      immutable: false,
      footnote: null,
      values: oldVal.spec.values.reduce(
        (obj, curr) => ({
          ...obj,
          [curr]: oldVal.spec['value-names'][curr],
        }),
        {},
      ),
    }
  } else if (isNumberList(oldVal)) {
    return {
      ...partial,
      type: 'list',
      default: oldVal.default.map(String) as string[],
      spec: {
        type: 'text',
        patterns: oldVal.spec.integral
          ? [{ regex: '[0-9]+', description: 'Integral number type' }]
          : [
              {
                regex: '[-+]?[0-9]*\\.?[0-9]+',
                description: 'Number type',
              },
            ],
        minLength: null,
        maxLength: null,
        masked: false,
        generate: null,
        inputmode: 'text',
        placeholder: oldVal.spec.placeholder
          ? String(oldVal.spec.placeholder)
          : null,
      },
    }
  } else if (isStringList(oldVal)) {
    return {
      ...partial,
      type: 'list',
      default: oldVal.default as string[],
      spec: {
        type: 'text',
        patterns:
          oldVal.spec.pattern && oldVal.spec['pattern-description']
            ? [
                {
                  regex: oldVal.spec.pattern,
                  description: oldVal.spec['pattern-description'],
                },
              ]
            : [],
        minLength: null,
        maxLength: null,
        masked: oldVal.spec.masked || false,
        generate: null,
        inputmode: 'text',
        placeholder: oldVal.spec.placeholder || null,
      },
    }
  } else if (isObjectList(oldVal)) {
    return {
      ...partial,
      type: 'list',
      default: oldVal.default as Record<string, unknown>[],
      spec: {
        type: 'object',
        spec: transformConfigSpec(matchOldConfigSpec.parse(oldVal.spec.spec)),
        uniqueBy: oldVal.spec['unique-by'] || null,
        displayAs: oldVal.spec['display-as'] || null,
      },
    }
  } else {
    throw new Error('Invalid list subtype. enum, string, and object permitted.')
  }
}

function isObject(val: OldValueSpec): val is OldValueSpecObject {
  return val.type === 'object'
}

function isUnion(val: OldValueSpec): val is OldValueSpecUnion {
  return val.type === 'union'
}

function isList(val: OldValueSpec): val is OldValueSpecList {
  return val.type === 'list'
}

function isPointer(val: OldValueSpec): val is OldValueSpecPointer {
  return val.type === 'pointer'
}

function isEnumList(
  val: OldValueSpecList,
): val is OldValueSpecList & { subtype: 'enum' } {
  return val.subtype === 'enum'
}

function isStringList(
  val: OldValueSpecList,
): val is OldValueSpecList & { subtype: 'string' } {
  return val.subtype === 'string'
}
function isNumberList(
  val: OldValueSpecList,
): val is OldValueSpecList & { subtype: 'number' } {
  return val.subtype === 'number'
}
function isObjectList(
  val: OldValueSpecList,
): val is OldValueSpecList & { subtype: 'object' } {
  return val.subtype === 'object'
}
function isUnionList(
  val: OldValueSpecList,
): val is OldValueSpecList & { subtype: 'union' } {
  return val.subtype === 'union'
}

export type OldConfigSpec = Record<string, OldValueSpec>
export const matchOldConfigSpec: z.ZodType<OldConfigSpec> = z.lazy(() =>
  z.record(z.string(), matchOldValueSpec),
)
export const matchOldDefaultString = z.union([
  z.string(),
  z.object({ charset: z.string(), len: z.number() }),
])
type OldDefaultString = z.infer<typeof matchOldDefaultString>

export const matchOldValueSpecString = z.object({
  type: z.enum(['string']),
  name: z.string(),
  masked: z.boolean().nullable().optional(),
  copyable: z.boolean().nullable().optional(),
  nullable: z.boolean().nullable().optional(),
  placeholder: z.string().nullable().optional(),
  pattern: z.string().nullable().optional(),
  'pattern-description': z.string().nullable().optional(),
  default: matchOldDefaultString.nullable().optional(),
  textarea: z.boolean().nullable().optional(),
  description: z.string().nullable().optional(),
  warning: z.string().nullable().optional(),
})

export const matchOldValueSpecNumber = z.object({
  type: z.enum(['number']),
  nullable: z.boolean(),
  name: z.string(),
  range: z.string(),
  integral: z.boolean(),
  default: z.number().nullable().optional(),
  description: z.string().nullable().optional(),
  warning: z.string().nullable().optional(),
  units: z.string().nullable().optional(),
  placeholder: z.union([z.number(), z.string()]).nullable().optional(),
})
type OldValueSpecNumber = z.infer<typeof matchOldValueSpecNumber>

export const matchOldValueSpecBoolean = z.object({
  type: z.enum(['boolean']),
  default: z.boolean(),
  name: z.string(),
  description: z.string().nullable().optional(),
  warning: z.string().nullable().optional(),
})
type OldValueSpecBoolean = z.infer<typeof matchOldValueSpecBoolean>

type OldValueSpecObject = {
  type: 'object'
  spec: OldConfigSpec
  name: string
  description?: string | null
  warning?: string | null
}
const matchOldValueSpecObject: z.ZodType<OldValueSpecObject> = z.object({
  type: z.enum(['object']),
  spec: z.lazy(() => matchOldConfigSpec),
  name: z.string(),
  description: z.string().nullable().optional(),
  warning: z.string().nullable().optional(),
})

const matchOldValueSpecEnum = z.object({
  values: z.array(z.string()),
  'value-names': z.record(z.string(), z.string()),
  type: z.enum(['enum']),
  default: z.string(),
  name: z.string(),
  description: z.string().nullable().optional(),
  warning: z.string().nullable().optional(),
})
type OldValueSpecEnum = z.infer<typeof matchOldValueSpecEnum>

const matchOldUnionTagSpec = z.object({
  id: z.string(), // The name of the field containing one of the union variants
  'variant-names': z.record(z.string(), z.string()), // The name of each variant
  name: z.string(),
  description: z.string().nullable().optional(),
  warning: z.string().nullable().optional(),
})
type OldValueSpecUnion = {
  type: 'union'
  tag: z.infer<typeof matchOldUnionTagSpec>
  variants: Record<string, OldConfigSpec>
  default: string
}
const matchOldValueSpecUnion: z.ZodType<OldValueSpecUnion> = z.object({
  type: z.enum(['union']),
  tag: matchOldUnionTagSpec,
  variants: z.record(
    z.string(),
    z.lazy(() => matchOldConfigSpec),
  ),
  default: z.string(),
})

type OldUniqueBy =
  | null
  | string
  | { any: OldUniqueBy[] }
  | { all: OldUniqueBy[] }

const matchOldUniqueBy: z.ZodType<OldUniqueBy> = z.lazy(() =>
  z.union([
    z.null(),
    z.string(),
    z.object({ any: z.array(matchOldUniqueBy) }),
    z.object({ all: z.array(matchOldUniqueBy) }),
  ]),
)

type OldListValueSpecObject = {
  spec: OldConfigSpec
  'unique-by'?: OldUniqueBy | null
  'display-as'?: string | null
}
const matchOldListValueSpecObject: z.ZodType<OldListValueSpecObject> = z.object(
  {
    spec: z.lazy(() => matchOldConfigSpec), // this is a mapped type of the config object at this level, replacing the object's values with specs on those values
    'unique-by': matchOldUniqueBy.nullable().optional(), // indicates whether duplicates can be permitted in the list
    'display-as': z.string().nullable().optional(), // this should be a handlebars template which can make use of the entire config which corresponds to 'spec'
  },
)
type OldListValueSpecUnion = {
  'unique-by'?: OldUniqueBy | null
  'display-as'?: string | null
  tag: z.infer<typeof matchOldUnionTagSpec>
  variants: Record<string, OldConfigSpec>
}
const matchOldListValueSpecUnion: z.ZodType<OldListValueSpecUnion> = z.object({
  'unique-by': matchOldUniqueBy.nullable().optional(),
  'display-as': z.string().nullable().optional(),
  tag: matchOldUnionTagSpec,
  variants: z.record(
    z.string(),
    z.lazy(() => matchOldConfigSpec),
  ),
})
const matchOldListValueSpecString = z.object({
  masked: z.boolean().nullable().optional(),
  copyable: z.boolean().nullable().optional(),
  pattern: z.string().nullable().optional(),
  'pattern-description': z.string().nullable().optional(),
  placeholder: z.string().nullable().optional(),
})

const matchOldListValueSpecEnum = z.object({
  values: z.array(z.string()),
  'value-names': z.record(z.string(), z.string()),
})
const matchOldListValueSpecNumber = z.object({
  range: z.string(),
  integral: z.boolean(),
  units: z.string().nullable().optional(),
  placeholder: z.union([z.number(), z.string()]).nullable().optional(),
})

type OldValueSpecListBase = {
  type: 'list'
  range: string
  default: string[] | number[] | OldDefaultString[] | Record<string, unknown>[]
  name: string
  description?: string | null
  warning?: string | null
}

type OldValueSpecList = OldValueSpecListBase &
  (
    | { subtype: 'string'; spec: z.infer<typeof matchOldListValueSpecString> }
    | { subtype: 'enum'; spec: z.infer<typeof matchOldListValueSpecEnum> }
    | { subtype: 'object'; spec: OldListValueSpecObject }
    | { subtype: 'number'; spec: z.infer<typeof matchOldListValueSpecNumber> }
    | { subtype: 'union'; spec: OldListValueSpecUnion }
  )

// represents a spec for a list
export const matchOldValueSpecList: z.ZodType<OldValueSpecList> =
  z.intersection(
    z.object({
      type: z.enum(['list']),
      range: z.string(), // '[0,1]' (inclusive) OR '[0,*)' (right unbounded), normal math rules
      default: z.union([
        z.array(z.string()),
        z.array(z.number()),
        z.array(matchOldDefaultString),
        z.array(z.object({}).passthrough()),
      ]),
      name: z.string(),
      description: z.string().nullable().optional(),
      warning: z.string().nullable().optional(),
    }),
    z.union([
      z.object({
        subtype: z.enum(['string']),
        spec: matchOldListValueSpecString,
      }),
      z.object({
        subtype: z.enum(['enum']),
        spec: matchOldListValueSpecEnum,
      }),
      z.object({
        subtype: z.enum(['object']),
        spec: matchOldListValueSpecObject,
      }),
      z.object({
        subtype: z.enum(['number']),
        spec: matchOldListValueSpecNumber,
      }),
      z.object({
        subtype: z.enum(['union']),
        spec: matchOldListValueSpecUnion,
      }),
    ]),
  ) as unknown as z.ZodType<OldValueSpecList>

type OldValueSpecPointer = {
  type: 'pointer'
} & (
  | {
      subtype: 'package'
      target: 'tor-key' | 'tor-address' | 'lan-address'
      'package-id': string
      interface: string
    }
  | {
      subtype: 'package'
      target: 'config'
      'package-id': string
      selector: string
      multi: boolean
    }
)
const matchOldValueSpecPointer: z.ZodType<OldValueSpecPointer> = z.intersection(
  z.object({
    type: z.literal('pointer'),
  }),
  z.union([
    z.object({
      subtype: z.literal('package'),
      target: z.enum(['tor-key', 'tor-address', 'lan-address']),
      'package-id': z.string(),
      interface: z.string(),
    }),
    z.object({
      subtype: z.literal('package'),
      target: z.enum(['config']),
      'package-id': z.string(),
      selector: z.string(),
      multi: z.boolean(),
    }),
  ]),
) as unknown as z.ZodType<OldValueSpecPointer>

type OldValueSpecString = z.infer<typeof matchOldValueSpecString>

type OldValueSpec =
  | OldValueSpecString
  | OldValueSpecNumber
  | OldValueSpecBoolean
  | OldValueSpecObject
  | OldValueSpecEnum
  | OldValueSpecList
  | OldValueSpecUnion
  | OldValueSpecPointer

export const matchOldValueSpec: z.ZodType<OldValueSpec> = z.union([
  matchOldValueSpecString,
  matchOldValueSpecNumber,
  matchOldValueSpecBoolean,
  matchOldValueSpecObject as z.ZodType<OldValueSpecObject>,
  matchOldValueSpecEnum,
  matchOldValueSpecList as z.ZodType<OldValueSpecList>,
  matchOldValueSpecUnion as z.ZodType<OldValueSpecUnion>,
  matchOldValueSpecPointer as z.ZodType<OldValueSpecPointer>,
])

export class Range {
  min?: number
  max?: number
  minInclusive!: boolean
  maxInclusive!: boolean

  static from(s: string = '(*,*)'): Range {
    const r = new Range()
    r.minInclusive = s.startsWith('[')
    r.maxInclusive = s.endsWith(']')
    const [minStr, maxStr] = s.split(',').map(a => a.trim())
    r.min = minStr === '(*' ? undefined : Number(minStr.slice(1))
    r.max = maxStr === '*)' ? undefined : Number(maxStr.slice(0, -1))
    return r
  }
}
