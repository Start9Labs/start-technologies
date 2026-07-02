import {
  AbstractControl,
  NonNullableFormBuilder,
  ValidationErrors,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { FormRawValue } from 'src/app/services/form.service'
import { tpl } from 'src/app/i18n/validation-errors'

export const FIRST_OCTETS = [192, 10, 172] as const

export type FirstOctet = (typeof FIRST_OCTETS)[number]

// Default (lowest valid) second octet for each first octet — used when the
// first octet changes or a parsed value falls outside the block's range.
export const SECOND_OCTET_MAP: Record<FirstOctet, number> = {
  192: 168, // 192.168.x.x (Class C private)
  10: 0, // 10.0.x.x (Class A private)
  172: 16, // 172.16.x.x (Class B private)
}

// Valid second-octet range per RFC 1918 block. The prefix length of each block
// bounds how many /16s it contains, i.e. how far the second octet can vary:
//   192.168.0.0/16 — exactly one /16, second octet locked to 168
//   172.16.0.0/12  — 16 /16s, second octet 16..31
//   10.0.0.0/8     — 256 /16s, second octet 0..255
export const SECOND_OCTET_RANGE: Record<
  FirstOctet,
  { min: number; max: number }
> = {
  192: { min: 168, max: 168 },
  10: { min: 0, max: 255 },
  172: { min: 16, max: 31 },
}

export const LAN_IPV4_VALIDATION_ERRORS = {
  required: 'Required',
  min: tpl<{ min: number }>('Minimum value is {min}', ({ min }) => ({ min })),
  max: tpl<{ max: number }>('Maximum value is {max}', ({ max }) => ({ max })),
}

export function getSecondOctet(first: FirstOctet): number {
  return SECOND_OCTET_MAP[first]
}

export function getSecondOctetRange(first: FirstOctet): {
  min: number
  max: number
} {
  return SECOND_OCTET_RANGE[first]
}

// Keep a second octet within its block's range, falling back to the block
// default when it's out of range or not a valid octet. Used only for load-time
// normalization in parseIpToForm — the backend guarantees stored values are
// in-block, so in practice this never alters a real value.
export function clampSecondOctet(first: FirstOctet, second: number): number {
  const { min, max } = SECOND_OCTET_RANGE[first]
  if (!Number.isInteger(second) || second < min || second > max) {
    return getSecondOctet(first)
  }
  return second
}

export function isSecondOctetInRange(
  first: FirstOctet,
  second: number,
): boolean {
  const { min, max } = SECOND_OCTET_RANGE[first]
  return Number.isInteger(second) && second >= min && second <= max
}

// The 192.168.0.0/16 block has a single legal second octet (168), so that field
// is never user-editable. Rather than imperatively force the form control's
// value, callers resolve it here: a single-value block collapses to its fixed
// octet, every other block keeps the user's choice. This lets the locked field
// be a pure display ([value]="…") instead of a force-set control.
export function resolveSecondOctet(first: FirstOctet, second: number): number {
  const { min, max } = SECOND_OCTET_RANGE[first]
  return min === max ? min : second
}

// Flags a second octet outside its block's range (rather than snapping it),
// so the form can block saving and show the allowed values. Mirrors
// CustomValidators.prefix in src/app/utils/validators.ts.
export function secondOctetBlockValidator(
  min: number,
  max: number,
): ValidatorFn {
  return (control: AbstractControl): ValidationErrors | null => {
    const value = control.value
    if (value == null) return null // 'required' handles empty
    return Number.isInteger(value) && value >= min && value <= max
      ? null
      : { secondOctetBlock: { min, max } }
  }
}

export function getLanIpv4Form(builder: NonNullableFormBuilder) {
  return builder.group({
    ip: builder.group({
      firstOctet: builder.control<FirstOctet>(192),
      // Block-range validation is declarative (the `[tuiValidator]` directive in
      // form/ip.ts, since the allowed range depends on the selected first
      // octet); only `required` is static here.
      secondOctet: builder.control(getSecondOctet(192), Validators.required),
      // The 3rd octet is no longer editable here — the Admin Security Profile's
      // subnet field is the single source of truth for it (and enforces the
      // per-/24 collision guard that this form lacked). It's kept in the model
      // (populated from the loaded IP, no UI input) so a network-block change
      // preserves the current subnet and the summary can show the router IP.
      routerOctet: builder.control(1),
    }),
  })
}

export type LanIpv4Form = FormRawValue<ReturnType<typeof getLanIpv4Form>>

export function buildNetworkBlock(ip: LanIpv4Form['ip']): string {
  const second = resolveSecondOctet(ip.firstOctet, ip.secondOctet)
  return `${ip.firstOctet}.${second}.0.0/16`
}

export function buildRouterIp(ip: LanIpv4Form['ip']): string {
  const second = resolveSecondOctet(ip.firstOctet, ip.secondOctet)
  return `${ip.firstOctet}.${second}.${ip.routerOctet}.1`
}

export function parseIpToForm(ipAddr: string): LanIpv4Form {
  const [first, second, third] = ipAddr.split('.').map(Number)
  const firstOctet = (
    FIRST_OCTETS.includes(first as FirstOctet) ? first : 192
  ) as FirstOctet

  return {
    ip: {
      firstOctet,
      secondOctet: clampSecondOctet(firstOctet, second),
      routerOctet: third ?? 1,
    },
  }
}
