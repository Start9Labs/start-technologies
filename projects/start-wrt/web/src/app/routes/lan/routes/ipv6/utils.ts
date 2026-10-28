import {
  AbstractControl,
  FormGroup,
  NonNullableFormBuilder,
  ValidationErrors,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { FormRawValue } from 'src/app/services/form.service'
import { tpl } from 'src/app/i18n/validation-errors'

export function getLanIpv6Form(builder: NonNullableFormBuilder) {
  return builder.group({
    strategy: builder.group({
      slaac: builder.control(true),
      dhcpv6: builder.control(false),
    }),
    subnet: builder.group({
      prefix: builder.control('/64', [
        Validators.required,
        Validators.pattern(/^\/\d{1,3}$/),
      ]),
    }),
  })
}

export type LanIpv6Form = FormRawValue<ReturnType<typeof getLanIpv6Form>>

const MIN_PREFIX = 48
const MAX_PREFIX = 64

export function prefixValidator(delegatedPrefix: number | null): ValidatorFn {
  return (control: AbstractControl): ValidationErrors | null => {
    const value = control.value
    if (!value) return null

    const lanPrefix = prefixToNumber(value)
    if (lanPrefix < MIN_PREFIX || lanPrefix > MAX_PREFIX) {
      return { prefixRange: true }
    }
    if (
      delegatedPrefix !== null &&
      delegatedPrefix <= MAX_PREFIX &&
      lanPrefix < delegatedPrefix
    ) {
      return { delegatedPrefix: { delegatedPrefix } }
    }
    return null
  }
}

export const PREFIX_VALIDATION_ERRORS = {
  required: 'Required',
  pattern: 'Invalid prefix format',
  prefixRange: 'Enter a value from 48 to 64.',
  delegatedPrefix: tpl<{ delegatedPrefix: number }>(
    'Your ISP delegated a /{delegatedPrefix} prefix. Enter a value from {delegatedPrefix} to 64.',
    ({ delegatedPrefix }) => ({ delegatedPrefix }),
  ),
}

export function updateLanIpv6Validators(
  form: FormGroup<ReturnType<typeof getLanIpv6Form>['controls']>,
  slaacEnabled: boolean,
  delegatedPrefix: number | null,
): void {
  const prefix = form.controls.subnet.controls.prefix

  if (slaacEnabled) {
    prefix.setValidators([
      Validators.required,
      Validators.pattern(/^\/\d{1,3}$/),
      prefixValidator(delegatedPrefix),
    ])
  } else {
    prefix.clearValidators()
  }

  prefix.updateValueAndValidity()
}

export function prefixToNumber(prefix: string): number {
  return parseInt(prefix.replace('/', ''), 10) || 64
}

export function numberToPrefix(num: number): string {
  return `/${num}`
}
