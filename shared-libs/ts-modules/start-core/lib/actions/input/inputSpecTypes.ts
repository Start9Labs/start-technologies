import type { UploadLimits } from '../../osBindings'

/**
 * A record mapping field keys to their {@link ValueSpec} definitions.
 * This is the root shape of a dynamic form specification — it defines the complete set
 * of configurable fields for a service or action.
 */
export type InputSpec = Record<string, ValueSpec>
/**
 * The discriminator for all supported form field types.
 */
export type ValueType =
  | 'text'
  | 'textarea'
  | 'number'
  | 'color'
  | 'datetime'
  | 'toggle'
  | 'triState'
  | 'select'
  | 'multiselect'
  | 'list'
  | 'object'
  | 'file'
  | 'union'
  | 'hidden'
/** Union of all concrete form field spec types. Discriminate on the `type` field. */
export type ValueSpec = ValueSpecOf<ValueType>
/** core spec types. These types provide the metadata for performing validations */
// prettier-ignore
export type ValueSpecOf<T extends ValueType> =
  T extends "text" ? ValueSpecText :
  T extends "textarea" ? ValueSpecTextarea :
  T extends "number" ? ValueSpecNumber :
  T extends "color" ? ValueSpecColor :
  T extends "datetime" ? ValueSpecDatetime :
  T extends "toggle" ? ValueSpecToggle :
  T extends "triState" ? ValueSpecTriState :
  T extends "select" ? ValueSpecSelect :
  T extends "multiselect" ? ValueSpecMultiselect :
  T extends "list" ? ValueSpecList :
  T extends "object" ? ValueSpecObject :
  T extends "file" ? ValueSpecFile :
  T extends "union" ? ValueSpecUnion :
  T extends "hidden" ? ValueSpecHidden :
  never

/** Spec for a single-line text input field. */
export type ValueSpecText = {
  /** Display label for the field. */
  name: string
  /** Optional help text displayed below the field. */
  description: string | null
  /** Optional warning message displayed to the user. */
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null

  type: 'text'
  /** Regex patterns used to validate the input value. */
  patterns: Pattern[]
  /** Minimum character length, or `null` for no minimum. */
  minLength: number | null
  /** Maximum character length, or `null` for no maximum. */
  maxLength: number | null
  /** Whether the field should obscure input (e.g. for passwords). */
  masked: boolean

  /** HTML input mode hint for mobile keyboards. */
  inputmode: 'text' | 'email' | 'tel' | 'url'
  /** Placeholder text shown when the field is empty. */
  placeholder: string | null

  /** Whether the field must have a value. */
  required: boolean
  /** Default value, which may be a literal string or a {@link RandomString} generation spec. */
  default: DefaultString | null
  /** `false` if editable, or a string message explaining why the field is disabled. */
  disabled: false | string
  /** If set, provides a "generate" button that fills the field with a random string matching this spec. */
  generate: null | RandomString
  /** Whether the field value cannot be changed after initial configuration. */
  immutable: boolean
}
/** Spec for a multi-line textarea input field. */
export type ValueSpecTextarea = {
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null

  type: 'textarea'
  /** Regex patterns used to validate the input value. */
  patterns: Pattern[]
  placeholder: string | null
  minLength: number | null
  maxLength: number | null
  /** Minimum number of visible rows. */
  minRows: number
  /** Maximum number of visible rows before scrolling. */
  maxRows: number
  required: boolean
  default: string | null
  disabled: false | string
  immutable: boolean
}

/** Spec for a numeric input field. */
export type ValueSpecNumber = {
  type: 'number'
  /** Minimum allowed value, or `null` for unbounded. */
  min: number | null
  /** Maximum allowed value, or `null` for unbounded. */
  max: number | null
  /** Whether only whole numbers are accepted. */
  integer: boolean
  /** Step increment for the input spinner, or `null` for any precision. */
  step: number | null
  /** Display label for the unit (e.g. `"MB"`, `"seconds"`), shown next to the field. */
  units: string | null
  placeholder: string | null
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null
  required: boolean
  default: number | null
  disabled: false | string
  immutable: boolean
}
/** Spec for a browser-native color picker field. */
export type ValueSpecColor = {
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null

  type: 'color'
  required: boolean
  /** Default hex color string (e.g. `"#ff0000"`), or `null`. */
  default: string | null
  disabled: false | string
  immutable: boolean
}
/** Spec for a date, time, or datetime input field. */
export type ValueSpecDatetime = {
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null
  type: 'datetime'
  required: boolean
  /** Controls which kind of picker is displayed. */
  inputmode: 'date' | 'time' | 'datetime-local'
  /** Minimum selectable date/time as an ISO string, or `null`. */
  min: string | null
  /** Maximum selectable date/time as an ISO string, or `null`. */
  max: string | null
  default: string | null
  disabled: false | string
  immutable: boolean
}
/** Spec for a single-select field displayed as radio buttons in a modal. */
export type ValueSpecSelect = {
  /** Map of option keys to display labels. */
  values: Record<string, string>
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null
  type: 'select'
  default: string | null
  /** `false` if all enabled, a string disabling the whole field, or an array of disabled option keys. */
  disabled: false | string | string[]
  immutable: boolean
}
/** Spec for a multi-select field displayed as checkboxes in a modal. */
export type ValueSpecMultiselect = {
  /** Map of option keys to display labels. */
  values: Record<string, string>

  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null

  type: 'multiselect'
  /** Minimum number of selections required, or `null`. */
  minLength: number | null
  /** Maximum number of selections allowed, or `null`. */
  maxLength: number | null
  /** `false` if all enabled, a string disabling the whole field, or an array of disabled option keys. */
  disabled: false | string | string[]
  /** Array of option keys selected by default. */
  default: string[]
  immutable: boolean
}
/** Spec for a boolean toggle (on/off switch). */
export type ValueSpecToggle = {
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null

  type: 'toggle'
  default: boolean | null
  disabled: false | string
  immutable: boolean
}
/**
 * Spec for a three-state toggle — a boolean toggle with an additional neutral
 * middle position. Left outputs `false`, right outputs `true`, middle outputs `null`.
 * Rendered as three icon buttons (✕ / — / ✓).
 */
export type ValueSpecTriState = {
  name: string
  description: string | null
  warning: string | null
  /** Optional supplementary text rendered persistently beneath the field. */
  footnote: string | null

  type: 'triState'
  /** Initial selection: `false` (left), `true` (right), or `null` (middle). */
  default: boolean | null
  disabled: false | string
  immutable: boolean
}
/**
 * Spec for a discriminated union field — displays a dropdown for variant selection,
 * and each variant can have its own nested sub-form.
 */
export type ValueSpecUnion = {
  name: string
  description: string | null
  warning: string | null

  type: 'union'
  /** Map of variant keys to their display name and nested form spec. */
  variants: Record<
    string,
    {
      /** Display name for this variant in the dropdown. */
      name: string
      /** Nested form spec shown when this variant is selected. */
      spec: InputSpec
    }
  >
  /** `false` if all enabled, a string disabling the whole field, or an array of disabled variant keys. */
  disabled: false | string | string[]
  default: string | null
  immutable: boolean
}
/** Matches filename suffixes case-insensitively; an empty list permits any filename. */
export function matchesFileExtensions(
  name: string,
  extensions: readonly string[],
): boolean {
  return (
    !extensions.length ||
    extensions.some(extension =>
      name.toLowerCase().endsWith(extension.toLowerCase()),
    )
  )
}

/** Spec for a file upload input field. */
export type ValueSpecFile = UploadLimits & {
  /** The prepared upload capability, or null for a browser-only file picker. */
  uploadId: string | null
  name: string
  description: string | null
  warning: string | null
  type: 'file'
  /** Allowed file extensions (e.g. `[".pem", ".crt"]`). */
  extensions: string[]
  required: boolean
}
/** Spec for a collapsible grouping of nested fields (a "sub-form"). */
export type ValueSpecObject = {
  name: string
  description: string | null
  warning: string | null
  type: 'object'
  /** The nested form spec containing this object's fields. */
  spec: InputSpec
}
/** Spec for a hidden field — not displayed to the user but included in the form data. */
export type ValueSpecHidden = {
  type: 'hidden'
}
/** The two supported list item types. */
export type ListValueSpecType = 'text' | 'object'
/** Maps a {@link ListValueSpecType} to its concrete list item spec. */
// prettier-ignore
export type ListValueSpecOf<T extends ListValueSpecType> =
  T extends "text" ? ListValueSpecText :
  T extends "object" ? ListValueSpecObject :
  never
/** A list field spec — union of text-list and object-list variants. */
export type ValueSpecList = ValueSpecListOf<ListValueSpecType>
/**
 * Spec for a list field — an interface to add, remove, and edit items in an ordered collection.
 * The `spec` field determines whether list items are text strings or structured objects.
 */
export type ValueSpecListOf<T extends ListValueSpecType> = {
  name: string
  description: string | null
  warning: string | null
  type: 'list'
  /** The item spec — determines whether this is a list of text values or objects. */
  spec: ListValueSpecOf<T>
  /** Minimum number of items, or `null` for no minimum. */
  minLength: number | null
  /** Maximum number of items, or `null` for no maximum. */
  maxLength: number | null
  disabled: false | string
  /** Default list items to populate on creation. */
  default:
    | string[]
    | DefaultString[]
    | Record<string, unknown>[]
    | readonly string[]
    | readonly DefaultString[]
    | readonly Record<string, unknown>[]
}
/** A regex validation pattern with a human-readable description of what it enforces. */
export type Pattern = {
  /** The regex pattern string (without delimiters). */
  regex: string
  /** A user-facing explanation shown when validation fails (e.g. `"Must be a valid email"`). */
  description: string
}
/** Spec for text items within a list field. */
export type ListValueSpecText = {
  type: 'text'
  patterns: Pattern[]
  minLength: number | null
  maxLength: number | null
  masked: boolean

  generate: null | RandomString
  inputmode: 'text' | 'email' | 'tel' | 'url'
  placeholder: string | null
}
/** Spec for object items within a list field. */
export type ListValueSpecObject = {
  type: 'object'
  /** The form spec for each object item. */
  spec: InputSpec
  /** Defines how uniqueness is determined among list items. */
  uniqueBy: UniqueBy
  /** An expression used to generate the display string for each item in the list summary (e.g. a key path). */
  displayAs: string | null
}

/**
 * Describes how list items determine uniqueness.
 * - `null`: no uniqueness constraint
 * - `string`: unique by a specific field key
 * - `{ any: UniqueBy[] }`: unique if any of the sub-constraints match
 * - `{ all: UniqueBy[] }`: unique if all sub-constraints match together
 */
export type UniqueBy =
  | null
  | string
  | {
      any: readonly UniqueBy[] | UniqueBy[]
    }
  | {
      all: readonly UniqueBy[] | UniqueBy[]
    }
/** A default value that is either a literal string or a {@link RandomString} generation spec. */
export type DefaultString = string | RandomString
/** Spec for generating a random string — used for default passwords, API keys, etc. */
export type RandomString = {
  /** The character set to draw from (e.g. `"a-zA-Z0-9"`). */
  charset: string
  /** The length of the generated string. */
  len: number
}
/**
 * Type guard that narrows a {@link ValueSpec} to a {@link ValueSpecListOf} of a specific item type.
 *
 * @param t - The value spec to check
 * @param s - The list item type to narrow to (`"text"` or `"object"`)
 */
export function isValueSpecListOf<S extends ListValueSpecType>(
  t: ValueSpec,
  s: S,
): t is ValueSpecListOf<S> & { spec: ListValueSpecOf<S> } {
  return 'spec' in t && t.spec.type === s
}
