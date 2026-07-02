import {
  AbstractControl,
  AsyncValidatorFn,
  NonNullableFormBuilder,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { FormRawValue } from 'src/app/services/form.service'
import { OutboundVpn } from 'src/app/services/api/api.service'
import { CustomValidators } from 'src/app/utils/validators'

export const OUTBOUND_VALIDATION_ERRORS = {
  required: 'Required',
  invalidExtension: 'Please upload a .conf file',
  duplicateName: 'A VPN with this label already exists',
  interfaceNameLength: 'Label is too long (max ~12 characters)',
  min: 'MTU must be at least 1280',
  max: 'MTU must be at most 1500',
  notWireguard: 'This doesn’t look like a WireGuard .conf file',
  looksLikeOpenvpn: 'OpenVPN configs aren’t supported — only WireGuard',
}

function fileExtensionValidator(
  allowedExtensions: string[],
): (control: AbstractControl) => ValidationErrors | null {
  return (control: AbstractControl): ValidationErrors | null => {
    const file = control.value as File | null
    if (!file) return null

    const extension = file.name.split('.').pop()?.toLowerCase()
    if (!extension || !allowedExtensions.includes(extension)) {
      return { invalidExtension: { allowed: allowedExtensions.join(', ') } }
    }
    return null
  }
}

/**
 * Async validator that reads the dropped file and confirms it's a WireGuard
 * config. Gives the user instant feedback before submit; the backend
 * (`parse_wireguard_config`) remains the authoritative gate. A WireGuard .conf
 * is an INI file with `[Interface]` + `[Peer]` headers — OpenVPN has neither.
 */
function wireguardConfigValidator(): AsyncValidatorFn {
  return async (control: AbstractControl): Promise<ValidationErrors | null> => {
    const file = control.value as File | null
    if (!file) return null

    let text: string
    try {
      text = await file.text()
    } catch {
      // Unreadable file — defer to the backend rather than block on a guess.
      return null
    }

    const hasInterface = /^[ \t]*\[interface\][ \t\r]*$/im.test(text)
    const hasPeer = /^[ \t]*\[peer\][ \t\r]*$/im.test(text)
    if (hasInterface && hasPeer) return null

    // Distinguish OpenVPN (common mistake) for a more helpful message. These
    // directives don't collide with any WireGuard .conf key.
    const openvpnMarkers = [
      /^[ \t]*client[ \t\r]*$/im,
      /^[ \t]*dev[ \t]+(tun|tap)/im,
      /^[ \t]*remote[ \t]+\S/im,
      /^[ \t]*proto[ \t]+(udp|tcp)/im,
      /^[ \t]*auth-user-pass/im,
      /^[ \t]*cipher[ \t]/im,
      /<ca>|<cert>|<key>|<tls-auth>|<tls-crypt>/i,
    ]
    return openvpnMarkers.some(r => r.test(text))
      ? { looksLikeOpenvpn: true }
      : { notWireguard: true }
  }
}

const labelValidators = (existingLabels: string[]) => [
  Validators.required,
  CustomValidators.duplicateName(existingLabels),
  CustomValidators.interfaceNameLength('wg_', 15),
]

export function getOutboundVpnForm(
  builder: NonNullableFormBuilder,
  existingLabels: string[],
) {
  return builder.group({
    label: builder.control('', labelValidators(existingLabels)),
    target: builder.control('Internet', [Validators.required]),
    // Optional. null = inherit the kernel default. Bounds mirror the backend
    // (1280 is the IPv6-safe floor for flaky/obfuscated tunnels).
    mtu: builder.control<number | null>(null, [
      Validators.min(1280),
      Validators.max(1500),
    ]),
  })
}

export function getAddOutboundVpnForm(
  builder: NonNullableFormBuilder,
  existingLabels: string[],
) {
  return builder.group({
    label: builder.control('', labelValidators(existingLabels)),
    config: builder.control<File | null>(null, {
      validators: [Validators.required, fileExtensionValidator(['conf'])],
      asyncValidators: [wireguardConfigValidator()],
    }),
    target: builder.control('Internet', [Validators.required]),
  })
}

export interface AddClientDialogData {
  targets: string[]
  existingLabels: string[]
}

export type OutboundVpnForm = FormRawValue<
  ReturnType<typeof getOutboundVpnForm>
>
export type AddOutboundVpnForm = FormRawValue<
  ReturnType<typeof getAddOutboundVpnForm>
>

/**
 * Compute which VPN labels are safe targets for a given VPN.
 * A target T is unsafe if following T's chain eventually reaches selfLabel (cycle).
 */
export function getSafeTargets(
  selfLabel: string,
  allVpns: OutboundVpn[],
): string[] {
  return [
    'Internet',
    ...allVpns
      .filter(v => v.label !== selfLabel)
      .filter(v => {
        const visited = new Set<string>([selfLabel])
        let current: OutboundVpn | undefined = v
        while (current && current.target !== 'Internet') {
          if (visited.has(current.target)) return false
          visited.add(current.target)
          current = allVpns.find(x => x.label === current!.target)
        }
        return true
      })
      .map(v => v.label),
  ]
}

/**
 * Build the full connection path for a VPN
 * e.g., ['Mullvad', 'Proton', 'Internet']
 */
export function buildConnectionPath(
  vpnLabel: string,
  allVpns: OutboundVpn[],
): string[] {
  const path: string[] = [vpnLabel]
  let current = allVpns.find(v => v.label === vpnLabel)

  // Follow the chain until we hit Internet or a cycle
  const visited = new Set<string>([vpnLabel])
  while (current && current.target !== 'Internet') {
    if (visited.has(current.target)) {
      // Cycle detected - shouldn't happen but handle gracefully
      path.push('...')
      break
    }
    visited.add(current.target)
    path.push(current.target)
    current = allVpns.find(v => v.label === current!.target)
  }

  path.push('Internet')
  return path
}
