import { inject, Injectable, signal } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { fill } from 'src/app/i18n/validation-errors'
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
  PublishedPortsSetRequest,
  RouterPortCollision,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Injectable()
export class PublishedPortsService extends FormService<PublishedPortDisplay[]> {
  private readonly api = inject(ApiService)
  private readonly devicesApi = inject(DevicesApiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

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

  /**
   * The backend applies the request unless an unconfirmed forward captures a
   * port the router itself answers on from the WAN (remote access, SSH, VPN)
   * — then it reports the collisions and applies nothing. Surface those for
   * confirmation and re-save with the override set on the named ports, so the
   * question is asked once per port (the override is persisted). The re-save
   * recurses through this same path: a collision that appears between attempts
   * (the config changed while the dialog was open) gets its own prompt rather
   * than a silently unapplied save. Overriding save() here covers every call
   * site: dialog save, toggle, delete.
   */
  override async save(data: PublishedPortDisplay[]): Promise<boolean> {
    let pending: RouterPortCollision[] = []
    const ok = await this.actions.run(async () => {
      const result = await this.api.publishedPortsSet(this.buildRequest(data))
      pending = result.pending_router_port_collisions
      if (!pending.length) await this.refreshAndWait()
    })
    if (!ok || !pending.length) return ok
    if (!(await this.confirmRouterPortOverride(pending))) {
      return false
    }
    const ids = new Set(pending.map(c => c.id))
    return this.save(
      data.map(p => (ids.has(p.id) ? { ...p, overrideRouterPorts: true } : p)),
    )
  }

  /**
   * If `pending` is non-empty, prompt the user to confirm publishing port(s)
   * the router itself answers on from the WAN (remote access to its web
   * interface, SSH, or a VPN server) — the forward would capture that traffic.
   * Returns true when there is nothing to confirm or the user confirmed, false
   * when they cancelled.
   */
  private async confirmRouterPortOverride(
    pending: RouterPortCollision[],
  ): Promise<boolean> {
    if (!pending.length) return true
    const ports = [...new Set(pending.flatMap(c => c.router_ports))].join(', ')
    return firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Port Used by This Router'),
        data: {
          content: fill(
            this.i18n.transform(
              'Port(s) {ports} are used by this router itself — for remote access to its web interface, SSH, or a VPN server. Publishing them will send that traffic to the selected device instead, cutting those services off from outside your network. Publish anyway?',
            ),
            { ports },
          ),
          yes: this.i18n.transform('Publish Anyway'),
          no: this.i18n.transform('Cancel'),
        },
      }),
    ).catch(() => false)
  }

  async store(items: PublishedPortDisplay[]): Promise<void> {
    await this.api.publishedPortsSet(this.buildRequest(items))
  }

  private buildRequest(
    items: PublishedPortDisplay[],
  ): PublishedPortsSetRequest {
    return {
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
        override_router_ports: item.overrideRouterPorts ?? false,
      })),
    }
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
    overrideRouterPorts: p.override_router_ports,
    status: p.status,
    statusReason: p.status_reason ?? undefined,
    deviceName: p.device_name ?? undefined,
    deviceIpv4: p.device_ipv4 ?? undefined,
    deviceIpv6: p.device_ipv6 ?? undefined,
  }
}
