import {
  AbstractControl,
  NonNullableFormBuilder,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { FormRawValue } from 'src/app/services/form.service'
import { CustomValidators } from 'src/app/utils/validators'
import { tpl } from 'src/app/i18n/validation-errors'

export const IPV4_MODES = ['dhcp', 'pppoe', 'static'] as const

export const IPV4_PPPOE_CONTROLS = ['username', 'password', 'device'] as const
export const IPV4_STATIC_CONTROLS = ['wan', 'prefix', 'gateway'] as const

export const IPV4_LABELS: Record<
  | Ipv4Mode
  | (typeof IPV4_PPPOE_CONTROLS)[number]
  | (typeof IPV4_STATIC_CONTROLS)[number]
  | 'mask',
  string
> = {
  dhcp: 'DHCP',
  pppoe: 'PPPoE',
  static: 'Static',
  wan: 'WAN IP',
  username: 'Username',
  password: 'Password',
  device: 'Device',
  prefix: 'Subnet Prefix',
  gateway: 'Gateway IP',
  mask: 'Subnet Mask',
}

export const IPV4_VALIDATION_ERRORS = {
  required: 'Required',
  ipv4: 'Enter a valid IPv4 address',
  prefix: tpl<{ min: number; max: number }>(
    'Enter a value between {min} and {max}',
    ({ min, max }) => ({ min, max }),
  ),
  gatewaySubnet: 'Gateway must be on the same subnet as the WAN IP',
  gatewaySameAsWan: 'Gateway must be different from the WAN IP',
}

export type Ipv4Mode = (typeof IPV4_MODES)[number]

export function getWanIpv4Form(builder: NonNullableFormBuilder) {
  return builder.group({
    ip: builder.group(
      {
        mode: builder.control<Ipv4Mode>('dhcp'),
        // Static fields
        wan: builder.control('', [CustomValidators.ipv4()]),
        prefix: builder.control('', [CustomValidators.prefix(0, 32)]),
        gateway: builder.control('', [CustomValidators.ipv4()]),
        // PPPoE fields
        username: builder.control(''),
        password: builder.control(''),
        device: builder.control(''),
      },
      { validators: [gatewaySubnetValidator] },
    ),
  })
}

export type WanIpv4Form = FormRawValue<ReturnType<typeof getWanIpv4Form>>

export function updateIpv4Validators(
  form: ReturnType<typeof getWanIpv4Form>,
  mode: Ipv4Mode,
): void {
  const ip = form.controls.ip.controls

  // Clear all mode-specific required validators first
  ip.wan.clearValidators()
  ip.prefix.clearValidators()
  ip.gateway.clearValidators()
  ip.username.clearValidators()
  ip.password.clearValidators()

  // Re-add format validators
  ip.wan.addValidators([CustomValidators.ipv4()])
  ip.prefix.addValidators([CustomValidators.prefix(0, 32)])
  ip.gateway.addValidators([CustomValidators.ipv4()])

  // Add required validators based on mode
  if (mode === 'static') {
    ip.wan.addValidators([Validators.required])
    ip.prefix.addValidators([Validators.required])
    ip.gateway.addValidators([Validators.required])
  } else if (mode === 'pppoe') {
    ip.username.addValidators([Validators.required])
    ip.password.addValidators([Validators.required])
  }

  // Update validity
  ip.wan.updateValueAndValidity()
  ip.prefix.updateValueAndValidity()
  ip.gateway.updateValueAndValidity()
  ip.username.updateValueAndValidity()
  ip.password.updateValueAndValidity()
}

function ipv4ToNumber(ip: string): number | null {
  const match = ip.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/)
  if (!match) return null
  const octets = [match[1], match[2], match[3], match[4]].map(Number)
  if (octets.some(o => o > 255)) return null
  return (
    ((octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]) >>> 0
  )
}

function gatewaySubnetValidator(
  group: AbstractControl,
): ValidationErrors | null {
  const wan = group.get('wan')?.value
  const prefix = group.get('prefix')?.value
  const gateway = group.get('gateway')?.value
  const mode = group.get('mode')?.value

  if (mode !== 'static' || !wan || !prefix || !gateway) return null

  const wanNum = ipv4ToNumber(wan)
  const gwNum = ipv4ToNumber(gateway)
  if (wanNum === null || gwNum === null) return null

  if (wanNum === gwNum) {
    group.get('gateway')?.setErrors({ gatewaySameAsWan: true })
    return { gatewaySameAsWan: true }
  }

  const prefixNum = parseInt(prefix.replace('/', ''), 10)
  if (isNaN(prefixNum) || prefixNum < 0 || prefixNum > 32) return null

  const mask = prefixNum === 0 ? 0 : (0xffffffff << (32 - prefixNum)) >>> 0
  if ((wanNum & mask) !== (gwNum & mask)) {
    group.get('gateway')?.setErrors({ gatewaySubnet: true })
    return { gatewaySubnet: true }
  }

  return null
}

export function prefixFromNetmask(netmask: string | undefined): string {
  if (!netmask) return ''

  // Convert netmask to CIDR prefix (with slash for Maskito)
  const parts = netmask.split('.').map(Number)
  const binary = parts.map(p => p.toString(2).padStart(8, '0')).join('')
  const prefix = binary.split('1').length - 1

  return `/${prefix}`
}

export function netmaskFromPrefix(prefix: string): string {
  // Remove leading slash if present, handle number input
  const prefixNum = parseInt(prefix.replace('/', ''), 10)

  if (isNaN(prefixNum) || prefixNum < 0 || prefixNum > 32) {
    return ''
  }

  // Create a 32-bit mask
  const mask = (0xffffffff << (32 - prefixNum)) >>> 0

  // Convert to dotted decimal notation
  const octet1 = (mask >>> 24) & 0xff
  const octet2 = (mask >>> 16) & 0xff
  const octet3 = (mask >>> 8) & 0xff
  const octet4 = mask & 0xff

  return `${octet1}.${octet2}.${octet3}.${octet4}`
}
