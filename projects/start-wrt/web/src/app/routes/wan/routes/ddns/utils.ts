import { NonNullableFormBuilder, Validators } from '@angular/forms'
import { FormRawValue } from 'src/app/services/form.service'

// Provider configurations with their required fields
export const DDNS_PROVIDERS = {
  start9: {
    label: 'Start9',
    fields: [],
  },
  dyndns: {
    label: 'DynDNS',
    fields: ['username', 'password', 'hostname'],
  },
  noip: {
    label: 'No-IP',
    fields: ['username', 'password', 'hostname'],
  },
  cloudflare: {
    label: 'Cloudflare',
    fields: ['token', 'zone', 'hostname'],
  },
  duckdns: {
    label: 'DuckDNS',
    fields: ['token', 'hostname'],
  },
  freedns: {
    label: 'FreeDNS',
    fields: ['token', 'hostname'],
  },
} as const

export type DdnsProvider = keyof typeof DDNS_PROVIDERS

export const DDNS_PROVIDER_LIST = Object.keys(DDNS_PROVIDERS) as DdnsProvider[]

export const DDNS_FIELD_LABELS: Record<string, string> = {
  username: 'Username',
  password: 'Password',
  hostname: 'Hostname',
  token: 'API Token',
  zone: 'Zone ID',
}

export const DDNS_VALIDATION_ERRORS = {
  required: 'Required',
}

export function getDdnsForm(builder: NonNullableFormBuilder) {
  return builder.group({
    enabled: builder.control(false),
    provider: builder.control<DdnsProvider>('start9'),
    fields: builder.group({
      username: builder.control(''),
      password: builder.control(''),
      hostname: builder.control(''),
      token: builder.control(''),
      zone: builder.control(''),
    }),
  })
}

export type DdnsForm = FormRawValue<ReturnType<typeof getDdnsForm>>

export function getProviderFields(provider: DdnsProvider): string[] {
  return [...DDNS_PROVIDERS[provider].fields]
}

export function updateDdnsValidators(
  form: ReturnType<typeof getDdnsForm>,
  enabled: boolean,
  provider: DdnsProvider,
): void {
  const controls = form.controls.fields.controls

  // Clear all validators first
  controls.username.clearValidators()
  controls.password.clearValidators()
  controls.hostname.clearValidators()
  controls.token.clearValidators()
  controls.zone.clearValidators()

  if (enabled) {
    const fields = getProviderFields(provider)

    if (fields.includes('username')) {
      controls.username.addValidators([Validators.required])
    }
    if (fields.includes('password')) {
      controls.password.addValidators([Validators.required])
    }
    if (fields.includes('hostname')) {
      controls.hostname.addValidators([Validators.required])
    }
    if (fields.includes('token')) {
      controls.token.addValidators([Validators.required])
    }
    if (fields.includes('zone')) {
      controls.zone.addValidators([Validators.required])
    }
  }

  // Update validity
  controls.username.updateValueAndValidity()
  controls.password.updateValueAndValidity()
  controls.hostname.updateValueAndValidity()
  controls.token.updateValueAndValidity()
  controls.zone.updateValueAndValidity()
}
