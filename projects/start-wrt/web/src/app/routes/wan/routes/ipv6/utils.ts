import { NonNullableFormBuilder, Validators } from '@angular/forms'
import { CustomValidators } from 'src/app/utils/validators'
import { FormRawValue } from 'src/app/services/form.service'
import { tpl } from 'src/app/i18n/validation-errors'

export const IPV6_MODES = [
  'slaac',
  'dhcpv6',
  'static',
  '6rd',
  'disabled',
] as const

export const IPV6_SLAAC_CONTROLS = ['prefix'] as const
export const IPV6_DHCPV6_CONTROLS = ['prefix'] as const
export const IPV6_STATIC_CONTROLS = [
  'wan',
  'prefix',
  'gateway',
  'lan_prefix',
] as const
export const IPV6_SIXRD_CONTROLS = [
  'ip6prefix',
  'ip6prefixlen',
  'ip4prefixlen',
  'border',
] as const
export const IPV6_ALL_CONTROLS = Array.from(
  new Set([...IPV6_STATIC_CONTROLS, ...IPV6_SIXRD_CONTROLS]),
)

export const IPV6_CONTROLS: Record<Ipv6Mode, readonly string[]> = {
  slaac: IPV6_SLAAC_CONTROLS,
  dhcpv6: IPV6_DHCPV6_CONTROLS,
  static: IPV6_STATIC_CONTROLS,
  '6rd': IPV6_SIXRD_CONTROLS,
  disabled: [],
}

export const IPV6_LABELS: Record<
  | Ipv6Mode
  | (typeof IPV6_STATIC_CONTROLS)[number]
  | (typeof IPV6_SIXRD_CONTROLS)[number],
  string
> = {
  slaac: 'SLAAC',
  dhcpv6: 'DHCPv6',
  static: 'Static',
  '6rd': '6RD',
  disabled: 'Disabled',
  wan: 'WAN IP',
  prefix: 'IPv6 Prefix',
  gateway: 'Gateway IP',
  lan_prefix: 'LAN Prefix',
  ip6prefix: 'IPv6 Prefix',
  ip6prefixlen: 'IPv6 Prefix Length',
  ip4prefixlen: 'IPv4 Prefix Length',
  border: 'Border Relay IP',
}

export const IPV6_VALIDATION_ERRORS = {
  required: 'Required',
  ipv4: 'Enter a valid IPv4 address',
  ipv6: 'Enter a valid IPv6 address',
  prefix: tpl<{ min: number; max: number }>(
    'Enter a value between {min} and {max}',
    ({ min, max }) => ({ min, max }),
  ),
}

export type Ipv6Mode = (typeof IPV6_MODES)[number]

export function getWanIpv6Form(builder: NonNullableFormBuilder) {
  return builder.group({
    ip: builder.group({
      mode: builder.control<Ipv6Mode>('slaac'),
      // Static/SLAAC/DHCPv6 fields
      wan: builder.control('', [CustomValidators.ipv6()]),
      prefix: builder.control('', [CustomValidators.prefix(0, 128)]),
      gateway: builder.control('', [CustomValidators.ipv6()]),
      // Static mode LAN prefix (e.g. "2001:db8::/48")
      lan_prefix: builder.control(''),
      // 6RD fields
      ip6prefix: builder.control('', [CustomValidators.ipv6()]),
      ip6prefixlen: builder.control('', [CustomValidators.prefix(0, 128)]),
      ip4prefixlen: builder.control('', [CustomValidators.prefix(0, 32)]),
      border: builder.control('', [CustomValidators.ipv4()]),
    }),
  })
}

export type WanIpv6Form = FormRawValue<ReturnType<typeof getWanIpv6Form>>

export function updateIpv6Validators(
  form: ReturnType<typeof getWanIpv6Form>,
  mode: Ipv6Mode,
): void {
  const ip = form.controls.ip.controls

  // Clear all mode-specific required validators
  ip.wan.clearValidators()
  ip.prefix.clearValidators()
  ip.gateway.clearValidators()
  ip.lan_prefix.clearValidators()
  ip.ip6prefix.clearValidators()
  ip.ip6prefixlen.clearValidators()
  ip.ip4prefixlen.clearValidators()
  ip.border.clearValidators()

  // Re-add format validators
  ip.wan.addValidators([CustomValidators.ipv6()])
  ip.prefix.addValidators([CustomValidators.prefix(0, 128)])
  ip.gateway.addValidators([CustomValidators.ipv6()])
  ip.ip6prefix.addValidators([CustomValidators.ipv6()])
  ip.ip6prefixlen.addValidators([CustomValidators.prefix(0, 128)])
  ip.ip4prefixlen.addValidators([CustomValidators.prefix(0, 32)])
  ip.border.addValidators([CustomValidators.ipv4()])

  // Add required validators based on mode
  if (mode === 'static') {
    ip.wan.addValidators([Validators.required])
    ip.prefix.addValidators([Validators.required])
    ip.gateway.addValidators([Validators.required])
    // lan_prefix is optional for static mode
  } else if (mode === '6rd') {
    ip.ip6prefix.addValidators([Validators.required])
    ip.ip6prefixlen.addValidators([Validators.required])
    ip.ip4prefixlen.addValidators([Validators.required])
    ip.border.addValidators([Validators.required])
  }

  // Update validity
  ip.wan.updateValueAndValidity()
  ip.prefix.updateValueAndValidity()
  ip.gateway.updateValueAndValidity()
  ip.lan_prefix.updateValueAndValidity()
  ip.ip6prefix.updateValueAndValidity()
  ip.ip6prefixlen.updateValueAndValidity()
  ip.ip4prefixlen.updateValueAndValidity()
  ip.border.updateValueAndValidity()
}
