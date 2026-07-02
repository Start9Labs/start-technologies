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

export function prefixValidator(wanPrefix: number): ValidatorFn {
  return (control: AbstractControl): ValidationErrors | null => {
    const value = control.value
    if (!value) return null

    const lanPrefix = prefixToNumber(value)
    if (lanPrefix <= wanPrefix) {
      return { minPrefix: { wanPrefix } }
    }
    if (lanPrefix >= 128) {
      return { maxPrefix: true }
    }
    return null
  }
}

export const PREFIX_VALIDATION_ERRORS = {
  required: 'Required',
  pattern: 'Invalid prefix format',
  minPrefix: tpl<{ wanPrefix: number }>(
    'Your WAN prefix is /{wanPrefix}. Enter a value greater than {wanPrefix} and less than 128.',
    ({ wanPrefix }) => ({ wanPrefix }),
  ),
  maxPrefix: 'Value must be less than 128.',
}

export function updateLanIpv6Validators(
  form: FormGroup<ReturnType<typeof getLanIpv6Form>['controls']>,
  slaacEnabled: boolean,
  wanPrefix: number,
): void {
  const prefix = form.controls.subnet.controls.prefix

  if (slaacEnabled) {
    prefix.setValidators([
      Validators.required,
      Validators.pattern(/^\/\d{1,3}$/),
      prefixValidator(wanPrefix),
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
