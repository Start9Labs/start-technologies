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
  /** `ipv4` is a reservation DHCP applies. */
  ipv4Static?: boolean
  dataUsage?: number // GB
  speed?: { up: number | null; down: number | null }
}

export interface DeviceReservation {
  ipv4: string
  /** Unset when no profile's subnet holds it. */
  profile?: string
  /** DHCP gives it to the device on the profile it is on. */
  active: boolean
}

// Full device data for detail view
export interface Device extends DeviceTableItem {
  ipv4Static: boolean
  ipv4Reservations: DeviceReservation[]
  /** The name assigned in the router; `name` is resolved from elsewhere when unset. */
  customName?: string
  /** May auto-create port forwards via PCP/UPnP (default off). */
  allowAutoPortForward: boolean
  /** Ports it was recently refused for lacking that permission. */
  autoPortForwardRequests: string[]
  /** May publish DNS records into the router's resolver (default off). */
  allowDnsInjection: boolean
  /** Was recently refused DNS records for lacking that permission. */
  dnsInjectionRequested: boolean
  /** Its IPv6 router advertisements reach the rest of the LAN (default off). */
  allowRouterAdvertisements: boolean
  /** The router's address on the device's profile, as in `192.168.2.1/24`. */
  profileNetwork?: string
}

// Form for editing a device. IPv6 has no controls: the device chooses its own
// IPv6 address (SLAAC) — the router cannot reserve one.
export function getDeviceForm(builder: NonNullableFormBuilder) {
  return builder.group({
    name: builder.control('', [CustomValidators.hostname()]),
    allowAutoPortForward: builder.control(false),
    allowDnsInjection: builder.control(false),
    allowRouterAdvertisements: builder.control(false),
    ip: builder.group({
      ipv4Static: builder.control(false),
      ipv4: builder.control('', [CustomValidators.ipv4()]),
    }),
  })
}

export type DeviceForm = FormRawValue<ReturnType<typeof getDeviceForm>>

// Flat data structure for service update
export interface DeviceUpdateData {
  /** Omitted leaves the assigned name untouched; empty clears it. */
  name?: string
  ipv4Static: boolean
  ipv4: string
  /** Reservations to delete, by address. */
  removeIpv4Reservations?: string[]
}

/** A permission is undefined when it didn't change. */
export type DevicePermissionChanges = Partial<
  Pick<
    Device,
    'allowAutoPortForward' | 'allowDnsInjection' | 'allowRouterAdvertisements'
  >
>

/**
 * An unchanged reservation stays valid, so other edits still save. `taken`
 * holds the addresses other devices have reserved.
 */
export function updateDeviceValidators(
  form: ReturnType<typeof getDeviceForm>,
  ipv4Static: boolean,
  device: Device | null,
  taken: readonly string[],
): void {
  const { ipv4 } = form.controls.ip.controls

  ipv4.clearValidators()
  ipv4.addValidators([CustomValidators.ipv4()])
  if (ipv4Static) {
    const unchanged = device?.ipv4Static ? device.ipv4 : null
    ipv4.addValidators([
      Validators.required,
      control =>
        control.value !== unchanged && taken.includes(control.value)
          ? { reservedElsewhere: true }
          : null,
    ])
    if (device?.profileNetwork) {
      const inNetwork = CustomValidators.ipv4HostIn(device.profileNetwork)
      ipv4.addValidators(control =>
        control.value === unchanged ? null : inNetwork(control),
      )
    }
  }
  ipv4.updateValueAndValidity()
}

export const DEVICE_VALIDATION_ERRORS = {
  required: 'Required',
  hostname: 'Letters, digits, and hyphens only',
  ipv4: 'Invalid IPv4 address',
  outsideProfile: "Outside the subnet of this device's profile",
  notHost: 'Not a usable host address',
  reservedElsewhere: 'Reserved for another device',
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
