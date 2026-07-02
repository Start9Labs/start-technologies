import { NonNullableFormBuilder, Validators } from '@angular/forms'
import { FormRawValue } from 'src/app/services/form.service'
import { CustomValidators } from 'src/app/utils/validators'

// Device status
export type DeviceStatus = 'online' | 'offline'

// Device as shown in the table
export interface DeviceTableItem {
  mac: string | null // Unique ID (null for VPN peers)
  name: string // Custom name or hostname
  hostname: string // Original hostname from device
  status: DeviceStatus
  connection?: string // e.g., 'Ethernet', 'Wi-Fi 5GHz', 'Wi-Fi 2.4GHz', 'VPN Home'
  securityProfile?: string // e.g., 'Default', 'Kids', 'Guest'
  ipv4?: string
  ipv6?: string
  ipv4Static?: boolean
  ipv6Static?: boolean
  dataUsage?: number // GB
  speed?: { up: number; down: number } // MB/s (only for online)
}

// Full device data for detail view
export interface Device extends DeviceTableItem {
  ipv4Static: boolean
  ipv6Static: boolean
}

// Form for editing a device
export function getDeviceForm(builder: NonNullableFormBuilder) {
  return builder.group({
    name: builder.control('', [CustomValidators.hostname()]),
    ip: builder.group({
      ipv4Static: builder.control(false),
      ipv4: builder.control('', [CustomValidators.ipv4()]),
      ipv6Static: builder.control(false),
      ipv6: builder.control('', [CustomValidators.ipv6()]),
    }),
  })
}

export type DeviceForm = FormRawValue<ReturnType<typeof getDeviceForm>>

// Flat data structure for service update
export interface DeviceUpdateData {
  name: string
  ipv4Static: boolean
  ipv4: string
  ipv6Static: boolean
  ipv6: string
}

export function updateDeviceValidators(
  form: ReturnType<typeof getDeviceForm>,
  ipv4Static: boolean,
  ipv6Static: boolean,
): void {
  const { ipv4, ipv6 } = form.controls.ip.controls

  // IPv4 validators
  ipv4.clearValidators()
  ipv4.addValidators([CustomValidators.ipv4()])
  if (ipv4Static) {
    ipv4.addValidators([Validators.required])
  }
  ipv4.updateValueAndValidity()

  // IPv6 validators
  ipv6.clearValidators()
  ipv6.addValidators([CustomValidators.ipv6()])
  if (ipv6Static) {
    ipv6.addValidators([Validators.required])
  }
  ipv6.updateValueAndValidity()
}

export const DEVICE_VALIDATION_ERRORS = {
  required: 'Required',
  hostname: 'Letters, digits, and hyphens only',
  ipv4: 'Invalid IPv4 address',
  ipv6: 'Invalid IPv6 address',
}

export const DEVICE_LABELS = {
  name: 'Name',
  mac: 'MAC Address',
  connection: 'Connection',
  securityProfile: 'Security Profile',
  ipv4: 'IPv4 Address',
  ipv6: 'IPv6 Address',
  dataUsage: 'Data Usage',
  speed: 'Speed',
} as const

// Data usage types
export type DataUsagePeriod = 'week' | 'month' | '3months'

export interface DataUsagePoint {
  timestamp: number // Unix timestamp (seconds)
  upload: number // bytes
  download: number // bytes
}

export interface DeviceDataUsage {
  mac: string
  period: DataUsagePeriod
  points: DataUsagePoint[]
}

export const DATA_USAGE_PERIOD_LABELS: Record<DataUsagePeriod, string> = {
  week: 'Last Week',
  month: 'Last 30 Days',
  '3months': 'Last 3 Months',
}
