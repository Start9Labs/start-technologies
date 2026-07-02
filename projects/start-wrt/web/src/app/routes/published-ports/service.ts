import { inject, Injectable, signal } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import {
  AutoForwardDisplay,
  PublishedPort,
  PublishedPortDisplay,
} from './types'
import { DevicesApiService } from 'src/app/routes/devices/service'
import { Device, DeviceUpdateData } from 'src/app/routes/devices/utils'
import {
  ApiService,
  AutoForwardFromApi,
  PublishedPortFromApi,
} from 'src/app/services/api/api.service'

@Injectable()
export class PublishedPortsService extends FormService<PublishedPortDisplay[]> {
  private readonly api = inject(ApiService)
  private readonly devicesApi = inject(DevicesApiService)

  private devices: Device[] = []

  /** Automatic (PCP/UPnP-created) forwards; refreshed alongside the manual list. */
  readonly autoForwards = signal<AutoForwardDisplay[]>([])

  async load(): Promise<PublishedPortDisplay[]> {
    // Load devices (for reserveDeviceIpv4) and both port lists in parallel
    const [devices, portsFromApi, autoFromApi] = await Promise.all([
      this.devicesApi.get(),
      this.api.publishedPortsList(),
      this.api.publishedPortsAutoList(),
    ])

    this.devices = devices
    this.autoForwards.set(autoFromApi.map(autoFromApiToDisplay))

    return portsFromApi.map(fromApiToDisplay)
  }

  async store(items: PublishedPortDisplay[]): Promise<void> {
    await this.api.publishedPortsSet({
      ports: items.map(item => ({
        id: item.id,
        enabled: item.enabled,
        label: item.label,
        device_mac: item.deviceMac,
        ports: item.ports,
        protocol: item.protocol,
        ipv4: item.ipv4,
        ipv6: item.ipv6,
        ipv4_public_port: item.ipv4PublicPort,
        source: item.source,
      })),
    })
  }

  getDevices(): Device[] {
    return this.devices
  }

  getDevice(mac: string): Device | undefined {
    return this.devices.find(d => d.mac?.toUpperCase() === mac.toUpperCase())
  }

  /**
   * Reserve the device's current IPv4 address as a static lease. There is no
   * IPv6 counterpart: the device chooses its own IPv6 address (SLAAC), so the
   * router cannot reserve one.
   */
  async reserveDeviceIpv4(mac: string): Promise<void> {
    const device = this.getDevice(mac)
    if (!device) return

    const updates: DeviceUpdateData = {
      name: device.name,
      ipv4Static: true,
      ipv4: device.ipv4 || '',
    }

    await this.devicesApi.update(mac, updates)

    device.ipv4Static = true
  }

  /**
   * Check if a device has any published ports
   */
  deviceHasPublishedPorts(mac: string): boolean {
    const data = this.data()
    if (!data) return false
    return data.some(p => p.deviceMac.toUpperCase() === mac.toUpperCase())
  }
}

function autoFromApiToDisplay(a: AutoForwardFromApi): AutoForwardDisplay {
  return {
    id: a.id,
    label: a.label,
    deviceMac: a.device_mac,
    deviceName: a.device_name ?? undefined,
    ports: a.ports,
    publicPorts: a.public_ports,
    expiresSecs: a.expires_secs ?? undefined,
  }
}

/** Map backend snake_case response to frontend camelCase types */
function fromApiToDisplay(p: PublishedPortFromApi): PublishedPortDisplay {
  return {
    id: p.id,
    enabled: p.enabled,
    label: p.label,
    deviceMac: p.device_mac,
    ports: p.ports,
    protocol: p.protocol,
    ipv4: p.ipv4,
    ipv6: p.ipv6,
    ipv4PublicPort: p.ipv4_public_port ?? undefined,
    source: p.source,
    status: p.status,
    statusReason: p.status_reason ?? undefined,
    deviceName: p.device_name ?? undefined,
    deviceIpv4: p.device_ipv4 ?? undefined,
    deviceIpv6: p.device_ipv6 ?? undefined,
  }
}
