import { Component, computed, inject, signal } from '@angular/core'
import { Router, RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiHint,
  TuiLink,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiSkeleton } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Placeholder } from 'src/app/components/placeholder'
import { ADD_SERVER, ServerDialogResult } from 'src/app/routes/inbound/dialog'
import {
  ADD_CLIENT,
  ClientDialogData,
} from 'src/app/routes/inbound/routes/client/dialog-add'
import { CLIENT_CONFIG } from 'src/app/routes/inbound/routes/client/dialog-config'
import { DevicesApiService } from 'src/app/routes/devices/service'
import {
  ApiService,
  ProfileId,
  VpnServerEndpoint,
  VpnServerPeer,
} from 'src/app/services/api/api.service'
import { InboundService, VpnServer } from 'src/app/routes/inbound/service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Inbound VPNs' | i18n }}</h2>
      </hgroup>
      <aside tuiAccessories>
        <button
          tuiButton
          iconStart="@tui.plus"
          [disabled]="!availableProfiles().length"
          [tuiHint]="
            !availableProfiles().length
              ? ('All security profiles already have inbound VPNs. Only one VPN can be created per profile.'
                | i18n)
              : null
          "
          (click)="edit()"
        >
          {{ 'Add' | i18n }}
        </button>
      </aside>
    </header>
    <table tuiTable class="g-table" [tuiSkeleton]="!service.data()">
      <thead>
        <tr>
          <th tuiTh [sorter]="'enabled' | tuiSorter" [style.width.rem]="3"></th>
          <th tuiTh [sorter]="'label' | tuiSorter">{{ 'Label' | i18n }}</th>
          <th tuiTh [sorter]="'endpoint' | tuiSorter">
            {{ 'Endpoint' | i18n }}
          </th>
          <th tuiTh [sorter]="'listen_port' | tuiSorter">
            {{ 'Port' | i18n }}
          </th>
          <th tuiTh [sorter]="'profile' | tuiSorter">
            {{ 'Security Profile' | i18n }}
          </th>
          <th tuiTh></th>
        </tr>
      </thead>
      <tbody>
        @for (item of service.data() | tuiTableSort; track item.profile) {
          <tr>
            <td tuiTd>{{ item.enabled ? '🟢' : '⚪' }}</td>
            <td tuiTd>
              <b>{{ item.label }}</b>
            </td>
            <td tuiTd>{{ item.endpoint }}</td>
            <td tuiTd>{{ item.listen_port }}</td>
            <td tuiTd>
              <a tuiLink routerLink="/profiles">
                {{ profileNameMap[item.profile] }}
              </a>
            </td>
            <td tuiTd>
              <button
                tuiIconButton
                size="xs"
                iconStart="@tui.ellipsis-vertical"
                appearance="icon"
                tuiDropdownAlign="end"
                tuiDropdownAuto
                tuiDropdown
              >
                {{ 'Actions' | i18n }}
                <tui-data-list
                  *tuiDropdown="let close"
                  size="s"
                  (click)="close()"
                >
                  <button
                    tuiOption
                    [iconStart]="
                      item.enabled ? '@tui.circle-pause' : '@tui.circle-play'
                    "
                    (click)="toggleEnabled(item)"
                  >
                    {{ (item.enabled ? 'Disable' : 'Enable') | i18n }}
                  </button>
                  <a
                    tuiOption
                    iconStart="@tui.monitor-smartphone"
                    routerLink="client"
                    [queryParams]="{ port: item.listen_port.toString() }"
                  >
                    {{ 'Manage clients' | i18n }}
                  </a>
                  <button
                    tuiOption
                    iconStart="@tui.pencil"
                    (click)="edit(item)"
                  >
                    {{ 'Edit' | i18n }}
                  </button>
                  <button
                    tuiOption
                    class="g-negative"
                    iconStart="@tui.trash"
                    (click)="delete(item.profile)"
                  >
                    {{ 'Delete' | i18n }}
                  </button>
                </tui-data-list>
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td tuiTd colspan="6">
              <app-placeholder icon="@tui.hard-drive-download">
                {{ 'No Inbound VPN Servers configured' | i18n }}
              </app-placeholder>
            </td>
          </tr>
        }
      </tbody>
    </table>
  `,
  styles: `
    :host {
      max-width: 50rem;
    }

    td:first-child {
      text-align: center;
    }

    td:last-child {
      text-align: end;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    TuiHeader,
    TuiTitle,
    TuiButton,
    TuiTable,
    TuiSkeleton,
    Placeholder,
    TuiLink,
    RouterLink,
    TuiDataList,
    TuiDropdown,
    TuiHint,
    i18nPipe,
  ],
})
export default class InboundTable {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly devices = inject(DevicesApiService)
  protected readonly dialogs = inject(TuiResponsiveDialogService)
  protected readonly service = inject(InboundService)
  private readonly i18n = inject(i18nPipe)

  private readonly profiles = signal<ProfileId[]>([])
  protected profileNameMap: Record<string, string> = {}
  private endpoints: VpnServerEndpoint[] = []

  protected readonly availableProfiles = computed(() => {
    const usedProfiles = new Set(
      (this.service.data() ?? []).map(s => s.profile),
    )
    return this.profiles().filter(p => !usedProfiles.has(p.interface))
  })

  constructor() {
    this.api.profilesList().then(list => {
      this.profiles.set(list)
      this.profileNameMap = Object.fromEntries(
        list.map(p => [p.interface, p.fullname]),
      )
    })
    this.fetchEndpoints()
  }

  private async fetchEndpoints() {
    const endpoints: VpnServerEndpoint[] = []

    const [ipv4, ipv6, ddns] = await Promise.all([
      this.api.wanIpv4Get(),
      this.api.wanIpv6Get(),
      this.api.wanDdnsGet(),
    ])

    if (ipv4.assigned_ip) {
      endpoints.push({ address: ipv4.assigned_ip, label: 'WAN IPv4' })
    }
    if (ipv6.mode !== 'disabled' && ipv6.assigned_ipv6) {
      endpoints.push({ address: ipv6.assigned_ipv6, label: 'WAN IPv6' })
    }
    if (ddns.enabled && ddns.hostname) {
      endpoints.push({ address: ddns.hostname, label: 'DDNS' })
    }

    this.endpoints = endpoints
  }

  delete(profile: string) {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() => {
        this.service.deleteServer(profile)
      })
  }

  toggleEnabled(item: VpnServer) {
    this.service.setServer(item.profile, {
      label: item.label,
      enabled: !item.enabled,
      listen_port: item.listen_port,
      endpoint: item.endpoint,
    })
  }

  private async addFirstClient(profile: string, listenPort: number) {
    const [devices, server] = await Promise.all([
      this.devices.get(),
      this.waitForServer(listenPort),
    ])

    const deviceIps = devices.map(d => d.ipv4).filter(Boolean) as string[]
    const usedIps = [...new Set([server.server_address, ...deviceIps])]

    this.dialogs
      .open<VpnServerPeer>(ADD_CLIENT, {
        label: this.i18n.transform('Add client device'),
        data: {
          serverAddress: server.server_address,
          usedIps,
        } satisfies ClientDialogData,
      })
      .subscribe(async peer => {
        const response = await this.service.addPeer(profile, peer)

        this.router.navigate(['/inbound', 'client'], {
          queryParams: { port: listenPort },
        })

        if (response?.client_config) {
          this.dialogs
            .open(CLIENT_CONFIG, {
              data: {
                name: peer.name,
                config: response.client_config,
              },
            })
            .subscribe()
        }
      })
  }

  private waitForServer(listenPort: number): Promise<VpnServer> {
    return new Promise(resolve => {
      const check = () => {
        const server = this.service
          .data()
          ?.find(s => s.listen_port === listenPort)
        if (server) {
          resolve(server)
        } else {
          setTimeout(check, 200)
        }
      }
      check()
    })
  }

  edit(data?: VpnServer) {
    const isEdit = !!data
    const profiles = isEdit ? this.profiles() : this.availableProfiles()

    if (!profiles.length) return

    this.dialogs
      .open<ServerDialogResult>(ADD_SERVER, {
        label: isEdit
          ? this.i18n.transform('Edit Inbound VPN')
          : this.i18n.transform('Add Inbound VPN'),
        data: {
          server: data,
          profiles,
          endpoints: this.endpoints,
          usedPorts: (this.service.data() ?? [])
            .filter(s => s.listen_port !== data?.listen_port)
            .map(s => s.listen_port),
        },
      })
      .subscribe(async response => {
        await this.service.setServer(response.profile, {
          label: response.label,
          enabled: response.enabled,
          listen_port: response.listen_port,
          endpoint: response.endpoint,
        })

        if (!isEdit) {
          this.addFirstClient(response.profile, response.listen_port)
        }
      })
  }
}
