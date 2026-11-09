import { inject, Injectable, signal } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  ApiService,
  DeviceLeavingProfile,
  EthernetConfig,
  EthernetSetConfig,
  ProfileId,
  UsbPortStatus,
} from 'src/app/services/api/api.service'

export interface EthernetPortView {
  name: string
  usb: UsbPortStatus | null
  profile: ProfileId | null
  profileName: string
  wan: boolean
}

@Injectable()
export class EthernetService extends FormService<EthernetPortView[]> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  /** Cached wan_ipv6 from last get, preserved across saves */
  private wanIpv6 = false

  /** Available profiles for the dropdown */
  readonly profiles = signal<ProfileId[]>([])

  async load(): Promise<EthernetPortView[]> {
    const [ethernet, profiles] = await Promise.all([
      this.api.ethernetGet(),
      this.api.profilesList(),
    ])

    this.wanIpv6 = ethernet.wan_ipv6
    this.profiles.set(profiles)

    return this.toPortViews(ethernet)
  }

  override async save(data: EthernetPortView[]): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Restarting network...'),
      success: this.i18n.transform('Ethernet settings saved'),
      restart: true,
    })
  }

  async store(items: EthernetPortView[]): Promise<void> {
    await this.api.ethernetSet(this.buildConfig(items, false))
  }

  /** Read fresh: the polled state trails a plug or unplug. */
  async usbPortInUse(port: string): Promise<boolean> {
    const { usb_ports } = await this.api.ethernetGet()
    return usb_ports[port]?.present ?? false
  }

  async removeUsbPort(port: string): Promise<void> {
    const removed = await this.actions.run(
      () => this.api.ethernetRemoveUsbPort({ port }),
      {
        loading: this.i18n.transform('Restarting network...'),
        success: this.i18n.transform('USB port removed'),
        fail: this.i18n.transform('Failed to remove USB port'),
        restart: true,
      },
    )
    if (removed) this.refresh()
  }

  /** The devices that keep settings for the profile a port leaves. */
  async previewPorts(
    items: EthernetPortView[],
  ): Promise<DeviceLeavingProfile[]> {
    const result = await this.api.ethernetSet(this.buildConfig(items, true))
    return result.leaving_profile
  }

  private buildConfig(
    items: EthernetPortView[],
    preview: boolean,
  ): EthernetSetConfig {
    const wanPort = items.find(p => p.wan)?.name ?? null
    return {
      wan_ipv6: this.wanIpv6,
      wan_port: wanPort,
      ports: Object.fromEntries(
        items.map(p => [p.name, { profile: p.wan ? null : p.profile }]),
      ),
      preview,
    }
  }

  private toPortViews(ethernet: EthernetConfig): EthernetPortView[] {
    return Object.entries(ethernet.ports)
      .map(([name, port]) => {
        const usb = ethernet.usb_ports[name] ?? null
        return {
          name,
          usb,
          profile: port.profile,
          profileName: port.profile?.fullname ?? 'Admin',
          wan: name === ethernet.wan_port,
        }
      })
      .sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { numeric: true }),
      )
  }
}
