import { inject, Injectable, signal } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  AffectedPublishedPort,
  ApiService,
  EthernetConfig,
  EthernetSetConfig,
  ProfileId,
} from 'src/app/services/api/api.service'

export interface EthernetPortView {
  name: string
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
    // store() is the real apply (via save()'s restart flow) — confirm the
    // published-port cleanup the preview already surfaced to the user.
    await this.api.ethernetSet(this.buildConfig(items, true))
  }

  /**
   * Dry-run: returns the published ports that would be deleted if these port
   * assignments were applied. Applies nothing (confirm = false).
   */
  async previewPorts(
    items: EthernetPortView[],
  ): Promise<AffectedPublishedPort[]> {
    const result = await this.api.ethernetSet(this.buildConfig(items, false))
    return result.pending_published_port_deletions
  }

  private buildConfig(
    items: EthernetPortView[],
    confirm: boolean,
  ): EthernetSetConfig {
    const wanPort = items.find(p => p.wan)?.name ?? null
    return {
      wan_ipv6: this.wanIpv6,
      wan_port: wanPort,
      ports: Object.fromEntries(
        items.map(p => [p.name, { profile: p.wan ? null : p.profile }]),
      ),
      confirm_published_port_deletion: confirm,
    }
  }

  private toPortViews(ethernet: EthernetConfig): EthernetPortView[] {
    return Object.entries(ethernet.ports)
      .map(([name, port]) => ({
        name,
        profile: port.profile,
        profileName: port.profile?.fullname ?? 'Admin',
        wan: name === ethernet.wan_port,
      }))
      .sort((a, b) => a.name.localeCompare(b.name))
  }
}
