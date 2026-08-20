import { inject, Injectable } from '@angular/core'
import {
  AbstractControl,
  FormArray,
  FormGroup,
  UntypedFormArray,
  UntypedFormBuilder,
  UntypedFormControl,
  UntypedFormGroup,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { IST, utils } from '@start9labs/start-core'
import { tuiCreateFileFormatValidator } from '@taiga-ui/kit'
import Mustache from 'mustache'

@Injectable({
  providedIn: 'root',
})
export class FormService {
  private readonly formBuilder = inject(UntypedFormBuilder)

  createForm(
    spec: IST.InputSpec,
    current: Record<string, any> = {},
  ): UntypedFormGroup {
    return this.getFormGroup(spec, [], current)
  }

  getUnionSelectSpec(
    spec: IST.ValueSpecUnion,
    selection: string | null,
  ): IST.ValueSpecSelect {
    return {
      ...spec,
      type: 'select',
      default: selection,
      footnote: null,
      values: Object.fromEntries(
        Object.entries(spec.variants).map(([key, { name }]) => [key, name]),
      ),
    }
  }

  getUnionObject(spec: IST.ValueSpecUnion, value: any): UntypedFormGroup {
    const valid = spec.variants[value?.selection]
    const selected = valid ? value?.selection : spec.default
    const selection = this.getUnionSelectSpec(spec, selected)
    const group = this.getFormGroup({ selection })
    const control = selected ? spec.variants[selected]?.spec : {}

    group.setControl(
      'value',
      this.getFormGroup(control || {}, [], value?.value),
    )

    return group
  }

  getListItem(spec: IST.ValueSpecList, entry?: any) {
    if (IST.isValueSpecListOf(spec, 'text')) {
      return this.formBuilder.control(entry, [
        ...stringValidators(spec.spec),
        Validators.required,
      ])
    } else if (IST.isValueSpecListOf(spec, 'object')) {
      return this.getFormGroup(spec.spec.spec, [], entry)
    }
  }

  getFormGroup(
    config: IST.InputSpec,
    validators: ValidatorFn[] = [],
    current?: Record<string, any> | null,
  ): UntypedFormGroup {
    let group: Record<
      string,
      UntypedFormGroup | UntypedFormArray | UntypedFormControl
    > = {}
    Object.entries(config).map(([key, spec]) => {
      group[key] = this.getFormEntry(spec, current?.[key])
    })
    return this.formBuilder.group(group, { validators })
  }

  private getFormEntry(
    spec: IST.ValueSpec,
    currentValue?: any,
  ): UntypedFormGroup | UntypedFormArray | UntypedFormControl {
    let value: any
    switch (spec.type) {
      case 'text':
        if (currentValue !== undefined) {
          value = currentValue
        } else {
          value = spec.default ? utils.getDefaultString(spec.default) : null
        }
        return this.formBuilder.control(value, stringValidators(spec))
      case 'textarea':
        if (currentValue !== undefined) {
          value = currentValue
        } else {
          value = spec.default || null
        }
        return this.formBuilder.control(value, stringValidators(spec))
      case 'number':
        if (currentValue !== undefined) {
          value = currentValue
        } else {
          value = spec.default || null
        }
        return this.formBuilder.control(value, numberValidators(spec))
      case 'color':
        if (currentValue !== undefined) {
          value = currentValue
        } else {
          value = spec.default || null
        }
        return this.formBuilder.control(value, colorValidators(spec))
      case 'datetime':
        if (currentValue !== undefined) {
          value = currentValue
        } else {
          value = spec.default || null
        }
        return this.formBuilder.control(value, datetimeValidators(spec))
      case 'object':
        return this.getFormGroup(spec.spec, [], currentValue)
      case 'list':
        const array = Array.isArray(currentValue) ? currentValue : spec.default
        const length = Math.max(array.length, spec.minLength || 0)

        return this.formBuilder.array(
          Array.from({ length }).map((_, index) =>
            this.getListItem(spec, array[index]),
          ),
          listValidators(spec),
        )
      case 'union':
        return this.getUnionObject(spec, currentValue)
      case 'toggle':
      case 'triState':
        value = currentValue === undefined ? spec.default : currentValue
        return this.formBuilder.control(value)
      case 'select':
        value = currentValue === undefined ? spec.default : currentValue
        return this.formBuilder.control(value, [Validators.required])
      case 'multiselect':
        value = currentValue === undefined ? spec.default : currentValue
        return this.formBuilder.control(value, multiselectValidators(spec))
      case 'hidden':
        return this.formBuilder.control(currentValue || null)
      case 'file':
        return this.formBuilder.control(null, fileValidators(spec))
      default:
        return this.formBuilder.control(null)
    }
  }
}

// function getListItemValidators(spec: IST.ValueSpecList) {
//   if (IST.isValueSpecListOf(spec, 'text')) {
//     return stringValidators(spec.spec)
//   }
// }

function stringValidators(
  spec: IST.ValueSpecText | IST.ValueSpecTextarea | IST.ListValueSpecText,
): ValidatorFn[] {
  const validators: ValidatorFn[] = []

  if ((spec as IST.ValueSpecText).required) {
    validators.push(Validators.required)
  }

  validators.push(textLengthInRange(spec.minLength, spec.maxLength))

  if (spec.patterns.length) {
    spec.patterns.forEach(p => validators.push(Validators.pattern(p.regex)))
  }

  return validators
}

function colorValidators({ required }: IST.ValueSpecColor): ValidatorFn[] {
  const validators: ValidatorFn[] = [Validators.pattern(/^#[0-9a-f]{6}$/i)]

  if (required) {
    validators.push(Validators.required)
  }

  return validators
}

function datetimeValidators({
  required,
  min,
  max,
}: IST.ValueSpecDatetime): ValidatorFn[] {
  const validators: ValidatorFn[] = []

  if (required) {
    validators.push(Validators.required)
  }

  if (min) {
    validators.push(datetimeMin(min))
  }

  if (max) {
    validators.push(datetimeMax(max))
  }

  return validators
}

function numberValidators(spec: IST.ValueSpecNumber): ValidatorFn[] {
  const validators: ValidatorFn[] = []

  validators.push(isNumber())

  if ((spec as IST.ValueSpecNumber).required) {
    validators.push(Validators.required)
  }

  if (spec.integer) {
    validators.push(isInteger())
  }

  validators.push(numberInRange(spec.min, spec.max))

  return validators
}

function multiselectValidators(spec: IST.ValueSpecMultiselect): ValidatorFn[] {
  const validators: ValidatorFn[] = []
  validators.push(listInRange(spec.minLength, spec.maxLength))
  return validators
}

function fileValidators(spec: IST.ValueSpecFile): ValidatorFn[] {
  const validators: ValidatorFn[] = [
    tuiCreateFileFormatValidator(spec.extensions.join(',')),
  ]

  if (spec.required) {
    validators.push(Validators.required)
  }

  return validators
}

function listValidators(spec: IST.ValueSpecList): ValidatorFn[] {
  const validators: ValidatorFn[] = []
  validators.push(listInRange(spec.minLength, spec.maxLength))
  validators.push(listItemIssue())
  return validators
}

// function fileValidators(spec: IST.ValueSpecFile): ValidatorFn[] {
//   const validators: ValidatorFn[] = []

//   if (spec.required) {
//     validators.push(Validators.required)
//   }

//   return validators
// }

export function numberInRange(
  min: number | null,
  max: number | null,
): ValidatorFn {
  return control => {
    const value = control.value
    if (typeof value !== 'number') return null
    if (min && value < min)
      return {
        numberNotInRange: `Number must be greater than or equal to ${min}`,
      }
    if (max && value > max)
      return { numberNotInRange: `Number must be less than or equal to ${max}` }
    return null
  }
}

export function isNumber(): ValidatorFn {
  return ({ value }) =>
    !value || value == Number(value) ? null : { notNumber: 'Must be a number' }
}

export function isInteger(): ValidatorFn {
  return ({ value }) =>
    !value || value == Math.trunc(value)
      ? null
      : { numberNotInteger: 'Must be an integer' }
}

export function listInRange(
  minLength: number | null,
  maxLength: number | null,
): ValidatorFn {
  return control => {
    const length = control.value.length
    if (minLength && length < minLength)
      return {
        listNotInRange: `List must contain at least ${minLength} entries`,
      }
    if (maxLength && length > maxLength)
      return {
        listNotInRange: `List cannot contain more than ${maxLength} entries`,
      }
    return null
  }
}

export function datetimeMin(min: string): ValidatorFn {
  return ({ value }) => {
    if (!value) return null

    const date = new Date(value.length === 5 ? `2000-01-01T${value}` : value)
    const minDate = new Date(min.length === 5 ? `2000-01-01T${min}` : min)

    return date < minDate ? { datetimeMin: `Minimum is ${min}` } : null
  }
}

export function datetimeMax(max: string): ValidatorFn {
  return ({ value }) => {
    if (!value) return null

    const date = new Date(value.length === 5 ? `2000-01-01T${value}` : value)
    const maxDate = new Date(max.length === 5 ? `2000-01-01T${max}` : max)

    return date > maxDate ? { datetimeMin: `Maximum is ${max}` } : null
  }
}

export function textLengthInRange(
  minLength: number | null,
  maxLength: number | null,
): ValidatorFn {
  return control => {
    const value = control.value
    if (value === null || value === undefined) return null

    const length = value.length
    if (minLength && length < minLength)
      return { listNotInRange: `Must be at least ${minLength} characters` }
    if (maxLength && length > maxLength)
      return { listNotInRange: `Cannot be great than ${maxLength} characters` }
    return null
  }
}

export function listItemIssue(): ValidatorFn {
  return parentControl => {
    const { controls } = parentControl as UntypedFormArray
    const problemChild = controls.find(c => c.invalid)
    if (problemChild) {
      return { listItemIssue: 'Invalid entries' }
    } else {
      return null
    }
  }
}

export function listUnique(spec: IST.ValueSpecList): ValidatorFn {
  return control => {
    const list = control.value
    for (let idx = 0; idx < list.length; idx++) {
      for (let idx2 = idx + 1; idx2 < list.length; idx2++) {
        if (listItemEquals(spec, list[idx], list[idx2])) {
          const objSpec = spec.spec
          let display1: string
          let display2: string
          let uniqueMessage = isListObject(objSpec)
            ? uniqueByMessageWrapper(objSpec.uniqueBy, objSpec)
            : ''

          if (isListObject(objSpec) && objSpec.displayAs) {
            display1 = `"${Mustache.render(objSpec.displayAs, list[idx])}"`
            display2 = `"${Mustache.render(objSpec.displayAs, list[idx2])}"`
          } else {
            display1 = `Entry ${idx + 1}`
            display2 = `Entry ${idx2 + 1}`
          }

          return {
            listNotUnique: `${display1} and ${display2} are not unique.${uniqueMessage}`,
          }
        }
      }
    }
    return null
  }
}

function listItemEquals(
  spec: IST.ValueSpecList,
  val1: any,
  val2: any,
): boolean {
  switch (spec.spec.type) {
    case 'text':
      return val1 == val2
    case 'object':
      const obj = spec.spec
      return listObjEquals(obj.uniqueBy, obj, val1, val2)
    default:
      return false
  }
}

function listObjEquals(
  uniqueBy: IST.UniqueBy,
  spec: IST.ListValueSpecObject,
  val1: any,
  val2: any,
): boolean {
  if (!uniqueBy) {
    return false
  } else if (typeof uniqueBy === 'string') {
    const uniqueBySpec = spec.spec[uniqueBy]

    return (
      !!uniqueBySpec &&
      uniqueByEquals(uniqueBySpec, val1[uniqueBy], val2[uniqueBy])
    )
  } else if ('any' in uniqueBy) {
    for (let unique of uniqueBy.any) {
      if (listObjEquals(unique, spec, val1, val2)) {
        return true
      }
    }
    return false
  } else if ('all' in uniqueBy) {
    for (let unique of uniqueBy.all) {
      if (!listObjEquals(unique, spec, val1, val2)) {
        return false
      }
    }
    return true
  }
  return false
}

function uniqueByEquals(spec: IST.ValueSpec, val1: any, val2: any): boolean {
  switch (spec.type) {
    case 'text':
    case 'textarea':
    case 'number':
    case 'toggle':
    case 'select':
    case 'color':
    case 'datetime':
      return val1 == val2
    case 'list':
      if (val1.length !== val2.length) {
        return false
      }
      for (let idx = 0; idx < val1.length; idx++) {
        if (listItemEquals(spec, val1[idx], val2[idx])) {
          return false
        }
      }
      return true
    default:
      return false
  }
}

function uniqueByMessageWrapper(
  uniqueBy: IST.UniqueBy,
  spec: IST.ListValueSpecObject,
) {
  let configSpec = spec.spec

  const message = uniqueByMessage(uniqueBy, configSpec)
  if (message) {
    return ' Must be unique by: ' + message
  }
}

function uniqueByMessage(
  uniqueBy: IST.UniqueBy,
  configSpec: IST.InputSpec,
  outermost = true,
): string {
  let joinFunc
  const subSpecs: string[] = []
  if (!uniqueBy) {
    return ''
  } else if (typeof uniqueBy === 'string') {
    return configSpec[uniqueBy]
      ? (configSpec[uniqueBy] as IST.ValueSpecObject).name
      : uniqueBy
  } else if ('any' in uniqueBy) {
    joinFunc = ' OR '
    for (let subSpec of uniqueBy.any) {
      subSpecs.push(uniqueByMessage(subSpec, configSpec, false))
    }
  } else if ('all' in uniqueBy) {
    joinFunc = ' AND '
    for (let subSpec of uniqueBy.all) {
      subSpecs.push(uniqueByMessage(subSpec, configSpec, false))
    }
  }
  const ret = subSpecs.filter(Boolean).join(joinFunc)
  return outermost || subSpecs.filter(ss => ss).length === 1
    ? ret
    : '(' + ret + ')'
}

function isListObject(
  spec: IST.ListValueSpecOf<any>,
): spec is IST.ListValueSpecObject {
  // only lists of objects have uniqueBy
  return 'uniqueBy' in spec
}

export function trim(control: AbstractControl) {
  if (control instanceof FormGroup || control instanceof FormArray) {
    Object.values(control.controls).forEach(trim)
  } else if (typeof control.value === 'string') {
    control.setValue(control.value.trim() || null)
  }
}
