import { inject, Injectable, signal } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { fill } from 'src/app/i18n/validation-errors'
import { FormService } from 'src/app/services/form.service'
import {
  AutomaticPortUseDisplay,
  PublishedPort,
  PublishedPortDisplay,
} from './types'
import { DevicesApiService } from 'src/app/routes/devices/service'
import { Device, DeviceUpdateData } from 'src/app/routes/devices/utils'
import {
  ApiService,
  AutomaticPortUseFromApi,
  PublishedPortFromApi,
  PublishedPortsSetRequest,
  WanPortCollision,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Injectable()
export class PublishedPortsService extends FormService<PublishedPortDisplay[]> {
  private readonly api = inject(ApiService)
  private readonly devicesApi = inject(DevicesApiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  private devices: Device[] = []

  readonly automaticPortUses = signal<AutomaticPortUseDisplay[]>([])

  async load(): Promise<PublishedPortDisplay[]> {
    const [devices, portsFromApi, autoFromApi] = await Promise.all([
      this.devicesApi.get(),
      this.api.publishedPortsList(),
      this.api.publishedPortsAutoList(),
    ])

    this.devices = devices
    this.automaticPortUses.set(
      autoFromApi.map(automaticPortUseFromApiToDisplay),
    )

    return portsFromApi.map(fromApiToDisplay)
  }

  override async save(data: PublishedPortDisplay[]): Promise<boolean> {
    let pending: WanPortCollision[] = []
    const ok = await this.actions.run(async () => {
      const result = await this.api.publishedPortsSet(this.buildRequest(data))
      pending = result.pending_wan_port_collisions
      if (!pending.length) await this.refreshAndWait()
    })
    if (!ok || !pending.length) return ok
    if (!(await this.confirmWanPortOverride(pending))) return false

    const ids = new Set(pending.map(c => c.id))
    return this.save(
      data.map(p => (ids.has(p.id) ? { ...p, overrideWanPorts: true } : p)),
    )
  }

  private async confirmWanPortOverride(
    pending: WanPortCollision[],
  ): Promise<boolean> {
    if (!pending.length) return true
    const routerPorts = [
      ...new Set(pending.flatMap(c => c.router_service_ports)),
    ]
    const sni = pending.flatMap(c => c.hostname_route_ports)
    const parts: string[] = []
    if (routerPorts.length) {
      parts.push(
        fill(
          this.i18n.transform(
            'Port(s) {ports} are used by this router itself — for remote access to its web interface, SSH, or a VPN server. Publishing them will send that traffic to the selected device instead, cutting those services off from outside your network.',
          ),
          { ports: routerPorts.join(', ') },
        ),
      )
    }
    if (sni.length) {
      const hostnames = [...new Set(sni.flatMap(s => s.hostnames))]
      const devices = [...new Set(sni.flatMap(s => s.devices))]
      const vars = {
        ports: [...new Set(sni.map(s => s.ports))].join(', '),
        list:
          hostnames.slice(0, 3).join(', ') +
          (hostnames.length > 3 ? ` (+${hostnames.length - 3})` : ''),
        devices: devices.join(', '),
      }
      parts.push(
        fill(
          this.i18n.transform(
            devices.length
              ? 'Port(s) {ports} currently carry hostname routes ({list}) registered by {devices}. Publishing them will send all traffic on these ports to the selected device instead — those hostname routes will stop working until this rule is removed.'
              : 'Port(s) {ports} currently carry hostname routes ({list}). Publishing them will send all traffic on these ports to the selected device instead — those hostname routes will stop working until this rule is removed.',
          ),
          vars,
        ),
      )
    }
    parts.push(this.i18n.transform('Publish anyway?'))
    return firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform(
          routerPorts.length
            ? 'Port Used by This Router'
            : 'Port Used for Hostname Routes',
        ),
        data: {
          content: parts.join(' '),
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
        override_wan_ports: item.overrideWanPorts ?? false,
      })),
    }
  }

  getDevices(): Device[] {
    return this.devices
  }

  getDevice(mac: string): Device | undefined {
    return this.devices.find(d => d.mac?.toUpperCase() === mac.toUpperCase())
  }

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

  deviceHasPublishedPorts(mac: string): boolean {
    const data = this.data()
    if (!data) return false
    return data.some(p => p.deviceMac.toUpperCase() === mac.toUpperCase())
  }
}

function automaticPortUseFromApiToDisplay(
  portUse: AutomaticPortUseFromApi,
): AutomaticPortUseDisplay {
  return {
    id: portUse.id,
    kind: portUse.kind,
    deviceMac: portUse.device_mac,
    deviceName: portUse.device_name ?? undefined,
    ports: portUse.ports,
    publicPorts: portUse.public_ports,
    expiresSecs: portUse.expires_secs ?? undefined,
    hostname: portUse.hostname ?? undefined,
  }
}

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
    overrideWanPorts: p.override_wan_ports,
    status: p.status,
    statusReason: p.status_reason ?? undefined,
    deviceName: p.device_name ?? undefined,
    deviceIpv4: p.device_ipv4 ?? undefined,
    deviceIpv6: p.device_ipv6 ?? undefined,
  }
}
