import { Component, computed, inject, signal } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from 'src/app/services/api/api.service'
import { provideFormService } from 'src/app/services/form.service'
import { PublishPortDialog } from './dialog'
import { PublishedPortsService } from './service'
import { PublishedPortsTable } from './table'
import { isGua, PublishedPortDialogResult, PublishedPortDisplay } from './types'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { confirmVpnExposedPort } from 'src/app/services/vpn-exposed-port'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Published Ports' | i18n }}</h2>
      </hgroup>
      <aside tuiAccessories>
        @if (!loading()) {
          <button tuiButton iconStart="@tui.plus" (click)="edit()">
            {{ 'Add' | i18n }}
          </button>
        }
      </aside>
    </header>
    <table
      [style.margin-block.rem]="1"
      [style.min-height.rem]="loading() ? 10 : 0"
      [publishedPorts]="loading() ? [] : service.data() || []"
      [ipv4EndpointHost]="ipv4EndpointHost()"
      [ipv6Available]="ipv6Available()"
      [vpnProfiles]="vpnProfiles()"
      [defaultProfile]="defaultProfile()"
      [tuiSkeleton]="loading()"
      (edit)="edit($event)"
    ></table>
  `,
  providers: [provideFormService(PublishedPortsService)],
  imports: [
    TuiHeader,
    TuiTitle,
    TuiButton,
    PublishedPortsTable,
    TuiSkeleton,
    i18nPipe,
  ],
})
export default class PublishedPorts {
  protected readonly dialogs = inject(TuiResponsiveDialogService)
  protected readonly service = inject(PublishedPortsService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  protected readonly loading = computed(() => !this.service.data())

  protected readonly ipv6Available = signal(true)

  // IPv4 endpoint host: DDNS hostname or WAN IP
  protected readonly ipv4EndpointHost = signal<string | null>(null)

  // Profiles whose outbound is a VPN, keyed by lower-cased fullname → VPN label.
  // Used to warn when a published port is created for a device whose profile
  // routes through a VPN (the port stays exposed on the public WAN IP).
  protected readonly vpnProfiles = signal<Map<string, string>>(new Map())
  // Fullname of the default (LAN-owning) profile — devices with no explicit
  // security profile belong to it.
  protected readonly defaultProfile = signal('')

  constructor() {
    this.loadDependencies()
  }

  private async loadDependencies() {
    const [wanIpv6, lanIpv6, ddns, wanIpv4] = await Promise.all([
      this.api.wanIpv6Get(),
      this.api.lanIpv6Get(),
      this.api.wanDdnsGet(),
      this.api.wanIpv4Get(),
    ])

    this.loadVpnProfiles()
    // IPv6 port forwarding requires WAN IPv6 + LAN IPv6, and crucially a real
    // global address delegated to the WAN — without a GUA prefix no LAN device
    // can be reachable, so ULA/link-local-only WANs must not offer IPv6.
    const wanGua = wanIpv6.assigned_ipv6
    const wanHasGua = wanGua ? isGua(wanGua) : false
    const lanIpv6Enabled = lanIpv6.slaac || lanIpv6.dhcpv6
    this.ipv6Available.set(wanHasGua && lanIpv6Enabled)

    // Use DDNS hostname if available, otherwise WAN IP
    const ddnsHostname = ddns.enabled ? ddns.hostname || null : null
    this.ipv4EndpointHost.set(ddnsHostname || wanIpv4.assigned_ip)
  }

  private async loadVpnProfiles() {
    try {
      const [profileIds, vpns] = await Promise.all([
        this.api.profilesList(),
        this.api.vpnClientList(),
      ])
      const profiles = await Promise.all(
        profileIds.map(id => this.api.profileGet(id)),
      )
      const vpnLabels = new Map(vpns.map(v => [v.id, v.label]))
      const map = new Map<string, string>()
      for (const p of profiles) {
        if (p.owns_lan) this.defaultProfile.set(p.fullname)
        if (p.outbound !== 'wan') {
          map.set(
            p.fullname.toLowerCase(),
            vpnLabels.get(p.outbound) ?? p.outbound,
          )
        }
      }
      this.vpnProfiles.set(map)
    } catch {
      // Non-fatal: without this the VPN-leak warning is simply not shown.
    }
  }

  edit(existing?: PublishedPortDisplay) {
    const devices = this.service.getDevices()
    const data = this.service.data() || []

    this.dialogs
      .open<PublishedPortDialogResult>(
        new PolymorpheusComponent(PublishPortDialog),
        {
          label: existing
            ? this.i18n.transform('Edit Published Port')
            : this.i18n.transform('Publish Ports'),
          data: {
            devices,
            existing,
            ipv6Available: this.ipv6Available(),
          },
        },
      )
      .subscribe(async result => {
        const { port: value, reserveIpv4, reserveIpv6 } = result

        // An enabled port whose device routes through a VPN stays exposed on the
        // public WAN IP — confirm before saving (on both create and edit).
        if (value.enabled) {
          const device = this.service.getDevice(value.deviceMac)
          const profile = device?.securityProfile || this.defaultProfile()
          const vpn = this.vpnProfiles().get(profile.toLowerCase())
          if (
            vpn &&
            !(await confirmVpnExposedPort(this.dialogs, this.i18n, {
              profile,
              vpn,
              labels: [value.label],
            }))
          ) {
            return
          }
        }

        // Handle IP reservation if needed
        if (reserveIpv4 || reserveIpv6) {
          this.service.reserveDeviceIps(
            value.deviceMac,
            reserveIpv4,
            reserveIpv6,
          )
        }

        // Update/enrich the port
        const device = this.service.getDevice(value.deviceMac)
        const port: PublishedPortDisplay = {
          ...value,
          status: value.enabled ? 'active' : 'disabled',
          deviceName: device?.name || device?.hostname,
          deviceIpv4: device?.ipv4,
          deviceIpv6: device?.ipv6,
        }

        // Update array
        const items = existing
          ? data.map(p => (p.id === value.id ? port : p))
          : data.concat(port)

        this.service.save(items)
      })
  }
}
