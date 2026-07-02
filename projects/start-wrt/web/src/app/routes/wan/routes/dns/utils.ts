import { NonNullableFormBuilder, ValidatorFn, Validators } from '@angular/forms'
import { DnsServer } from 'src/app/services/api/api.service'
import { FormRawValue } from 'src/app/services/form.service'

export const DNS_MODES = ['isp', 'custom'] as const

export type DnsMode = (typeof DNS_MODES)[number]

export const DNS_LABELS: Record<DnsMode, string> = {
  isp: 'Get from ISP',
  custom: 'Custom',
}

export const DNS_VALIDATION_ERRORS = {
  required: 'Required',
  ipv4: 'Enter a valid IPv4 address',
  ipv6: 'Enter a valid IPv6 address',
  ip: 'Enter a valid IP address',
}

export function getDnsForm(
  builder: NonNullableFormBuilder,
  validators: ValidatorFn[],
) {
  return builder.group({
    mode: builder.control<DnsMode>('isp'),
    custom1: builder.control('', validators),
    custom2: builder.control('', validators),
    custom3: builder.control('', validators),
    custom1Tls: builder.control(false),
    custom2Tls: builder.control(false),
    custom3Tls: builder.control(false),
  })
}

export function updateDnsValidators(
  form: ReturnType<typeof getDnsForm>,
  mode: DnsMode,
  formatValidators: ValidatorFn[],
): void {
  const { custom1, custom2, custom3 } = form.controls

  for (const control of [custom1, custom2, custom3]) {
    control.clearValidators()
    if (mode === 'custom') {
      control.addValidators(formatValidators)
    }
    control.updateValueAndValidity()
  }

  if (mode === 'custom') {
    custom1.addValidators([Validators.required])
    custom1.updateValueAndValidity()
  }
}

export type DnsForm = FormRawValue<ReturnType<typeof getDnsForm>>

export function dnsServersToForm(servers: DnsServer[]): Partial<DnsForm> {
  const s = (i: number) => servers[i] || { address: '', ssl: false }
  return {
    custom1: s(0).address,
    custom1Tls: s(0).ssl,
    custom2: s(1).address,
    custom2Tls: s(1).ssl,
    custom3: s(2).address,
    custom3Tls: s(2).ssl,
  }
}

export function formToDnsServers(form: DnsForm): DnsServer[] {
  const servers: DnsServer[] = []
  if (form.custom1)
    servers.push({ address: form.custom1, ssl: form.custom1Tls })
  if (form.custom2)
    servers.push({ address: form.custom2, ssl: form.custom2Tls })
  if (form.custom3)
    servers.push({ address: form.custom3, ssl: form.custom3Tls })
  return servers
}
