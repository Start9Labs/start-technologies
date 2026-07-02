import { Component, computed, inject, signal } from '@angular/core'
import { ActivatedRoute, RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiLink,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiBadge, TuiSkeleton } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Placeholder } from 'src/app/components/placeholder'
import { DevicesApiService } from 'src/app/routes/devices/service'
import { InboundService, VpnServerPeer } from '../../service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

import { CLIENT_CONFIG } from './dialog-config'
import { ADD_CLIENT, ClientDialogData } from './dialog-add'
import { RENAME_CLIENT } from './dialog-rename'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>
          <a
            tuiLink
            routerLink=".."
            appearance=""
            iconStart="@tui.chevron-left"
            [style.font]="'inherit'"
            [style.text-decoration]="'none'"
            [tuiSkeleton]="!server()"
          >
            {{ server()?.label }}
            {{ '(client devices)' | i18n }}
          </a>
        </h2>
      </hgroup>
      <aside tuiAccessories>
        <button
          tuiButton
          iconStart="@tui.plus"
          [disabled]="!deviceIps()"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </aside>
    </header>
    <table tuiTable class="g-table" [tuiSkeleton]="!server()">
      <thead>
        <tr>
          <th tuiTh [sorter]="'name' | tuiSorter">{{ 'Name' | i18n }}</th>
          <th tuiTh [sorter]="'ip' | tuiSorter">
            {{ 'LAN IP Address' | i18n }}
          </th>
          <th tuiTh>{{ 'Routing' | i18n }}</th>
          <th tuiTh></th>
        </tr>
      </thead>
      <tbody>
        @for (item of server()?.peers | tuiTableSort; track item.public_key) {
          <tr>
            <td tuiTd>{{ item.name }}</td>
            <td tuiTd>{{ item.ip }}</td>
            <td tuiTd>
              <span tuiBadge [appearance]="item.route_all ? 'info' : 'neutral'">
                {{ (item.route_all ? 'All traffic' : 'LAN only') | i18n }}
              </span>
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
                    iconStart="@tui.pencil"
                    (click)="rename(item)"
                  >
                    {{ 'Rename' | i18n }}
                  </button>
                  <button
                    tuiOption
                    iconStart="@tui.repeat"
                    (click)="changeRouting(item)"
                  >
                    {{
                      (item.route_all
                        ? 'Switch to LAN only'
                        : 'Switch to all traffic'
                      ) | i18n
                    }}
                  </button>
                  <button
                    tuiOption
                    iconStart="@tui.file-text"
                    (click)="viewConfig(item)"
                  >
                    {{ 'View Config' | i18n }}
                  </button>
                  <button
                    tuiOption
                    class="g-negative"
                    iconStart="@tui.trash"
                    (click)="delete(item)"
                  >
                    {{ 'Delete' | i18n }}
                  </button>
                </tui-data-list>
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td tuiTd colspan="4">
              <app-placeholder icon="@tui.monitor-smartphone">
                {{ 'No clients configured' | i18n }}
              </app-placeholder>
            </td>
          </tr>
        }
      </tbody>
    </table>
  `,
  styles: `
    :host {
      max-width: 36rem;
    }

    span {
      display: inline-block;
      min-width: 4rem;
      border-radius: var(--tui-radius-xs);
    }

    td:last-child {
      text-align: end;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    RouterLink,
    TuiHeader,
    TuiTitle,
    TuiLink,
    TuiButton,
    TuiTable,
    TuiSkeleton,
    TuiDataList,
    TuiDropdown,
    TuiBadge,
    Placeholder,
    i18nPipe,
  ],
})
export default class InboundClients {
  private readonly service = inject(InboundService)
  private readonly devices = inject(DevicesApiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly listenPort = Number(
    inject(ActivatedRoute).snapshot.queryParams['port'],
  )

  protected readonly deviceIps = signal<string[] | null>(null)
  protected readonly server = computed(() =>
    this.service.data()?.find(item => item.listen_port === this.listenPort),
  )

  constructor() {
    this.devices.get().then(devices => {
      this.deviceIps.set(devices.map(d => d.ipv4).filter(Boolean) as string[])
    })
  }

  add() {
    const server = this.server()
    if (!server) return

    const peerIps = server.peers.map(p => p.ip).filter(Boolean) as string[]
    const usedIps = [
      ...new Set([
        server.server_address,
        ...(this.deviceIps() ?? []),
        ...peerIps,
      ]),
    ]

    this.dialogs
      .open<VpnServerPeer>(ADD_CLIENT, {
        label: this.i18n.transform('Add client device'),
        data: {
          serverAddress: server.server_address,
          usedIps,
        } satisfies ClientDialogData,
      })
      .subscribe(async peer => {
        const response = await this.service.addPeer(server.profile, peer)
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

  rename(peer: VpnServerPeer) {
    const profile = this.server()?.profile
    if (!profile) return

    this.dialogs
      .open<string>(RENAME_CLIENT, {
        label: this.i18n.transform('Rename Device'),
        data: peer.name,
      })
      .subscribe(newName => {
        this.service.addPeer(profile, { ...peer, name: newName })
      })
  }

  viewConfig(peer: VpnServerPeer) {
    const server = this.server()
    if (!server) return

    const subnet = server.server_address.replace(/\.\d+$/, '')
    const gateway = `${subnet}.1`
    const allowedIPs = peer.route_all ? '0.0.0.0/0, ::/0' : `${subnet}.0/24`

    const config = [
      '[Interface]',
      'PrivateKey = <PRIVATE_KEY>',
      `Address = ${peer.ip}/32`,
      `DNS = ${gateway}`,
      '',
      '[Peer]',
      `PublicKey = ${server.public_key}`,
      `Endpoint = ${server.endpoint}:${server.listen_port}`,
      `AllowedIPs = ${allowedIPs}`,
    ].join('\n')

    this.dialogs
      .open(CLIENT_CONFIG, {
        data: {
          name: peer.name,
          config,
        },
      })
      .subscribe()
  }

  changeRouting(peer: VpnServerPeer) {
    const server = this.server()
    if (!server || !peer.public_key) return

    const newRouteAll = !peer.route_all

    this.dialogs
      .open(TUI_CONFIRM, {
        label: this.i18n.transform('Change routing mode?'),
        data: {
          content: `${this.i18n.transform('This will delete the existing peer and create a new one with')} ${newRouteAll ? this.i18n.transform('all traffic') : this.i18n.transform('LAN only')} ${this.i18n.transform('routing. You will need to reconfigure your device with the new config.')}`,
          yes: this.i18n.transform('Continue'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(async () => {
        await this.service.deletePeer(server.profile, peer.public_key!)

        // Refresh device IPs so the deleted peer's IP is no longer reserved
        const devices = await this.devices.get()
        this.deviceIps.set(devices.map(d => d.ipv4).filter(Boolean) as string[])

        const peerIps = server.peers
          .filter(p => p.public_key !== peer.public_key)
          .map(p => p.ip)
          .filter(Boolean) as string[]
        const usedIps = [
          ...new Set([
            server.server_address,
            ...(this.deviceIps() ?? []),
            ...peerIps,
          ]),
        ]

        this.dialogs
          .open<VpnServerPeer>(ADD_CLIENT, {
            label: this.i18n.transform('Reconfigure client device'),
            data: {
              serverAddress: server.server_address,
              usedIps,
              defaults: {
                name: peer.name,
                ip: peer.ip,
                route_all: newRouteAll,
              },
            } satisfies ClientDialogData,
          })
          .subscribe(async newPeer => {
            const response = await this.service.addPeer(server.profile, newPeer)
            if (response?.client_config) {
              this.dialogs
                .open(CLIENT_CONFIG, {
                  data: {
                    name: newPeer.name,
                    config: response.client_config,
                  },
                })
                .subscribe()
            }
          })
      })
  }

  delete(peer: VpnServerPeer) {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() => {
        const profile = this.server()?.profile
        if (profile && peer.public_key) {
          this.service.deletePeer(profile, peer.public_key)
        }
      })
  }
}
