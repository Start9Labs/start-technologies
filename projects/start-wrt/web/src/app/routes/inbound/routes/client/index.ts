import { Component, computed, inject } from '@angular/core'
import { ActivatedRoute, RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiLink,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiBadge, TuiSkeleton } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Placeholder } from 'src/app/components/placeholder'
import { InboundService, VpnServerPeer } from '../../service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

import { CLIENT_CONFIG, ClientConfigData } from './dialog-config'
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
          [disabled]="!server()"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </aside>
    </header>
    @if (hasLegacy()) {
      <div tuiNotification appearance="warning">
        {{
          'Devices marked “Old address” can lose their connection while they are also on this network. Reissue their config to fix it.'
            | i18n
        }}
      </div>
    }
    <table tuiTable class="g-table" [tuiSkeleton]="!server()">
      <thead>
        <tr>
          <th tuiTh [sorter]="'name' | tuiSorter">{{ 'Name' | i18n }}</th>
          <th tuiTh [sorter]="'ip' | tuiSorter">
            {{ 'IP Address' | i18n }}
          </th>
          <th tuiTh>{{ 'Routing' | i18n }}</th>
          <th tuiTh></th>
        </tr>
      </thead>
      <tbody>
        @for (item of server()?.peers | tuiTableSort; track item.public_key) {
          <tr>
            <td tuiTd>{{ item.name }}</td>
            <td tuiTd>
              {{ item.ip }}
              @if (item.legacy_address) {
                <span tuiBadge appearance="warning">
                  {{ 'Old address' | i18n }}
                </span>
              }
            </td>
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
                    iconStart="@tui.globe"
                    (click)="toggleDnsInjection(item)"
                  >
                    {{
                      (item.allow_dns_injection
                        ? 'Revoke DNS record publishing'
                        : 'Allow DNS record publishing'
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
                    iconStart="@tui.refresh-cw"
                    (click)="reissue(item)"
                  >
                    {{ 'Reissue config' | i18n }}
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
    TuiNotification,
    Placeholder,
    i18nPipe,
  ],
})
export default class InboundClients {
  private readonly service = inject(InboundService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly listenPort = Number(
    inject(ActivatedRoute).snapshot.queryParams['port'],
  )

  protected readonly server = computed(() =>
    this.service.data()?.find(item => item.listen_port === this.listenPort),
  )
  protected readonly hasLegacy = computed(() =>
    this.server()?.peers.some(peer => peer.legacy_address),
  )

  add() {
    const server = this.server()
    if (!server) return

    this.dialogs
      .open<VpnServerPeer>(ADD_CLIENT, {
        label: this.i18n.transform('Add client device'),
        data: {
          peerSubnet: server.peer_subnet,
          peers: server.peers,
        } satisfies ClientDialogData,
      })
      .subscribe(peer => this.addAndShowConfig(server.profile, peer))
  }

  rename(peer: VpnServerPeer) {
    const profile = this.server()?.profile
    const publicKey = peer.public_key
    if (!profile || !publicKey) return

    this.dialogs
      .open<string>(RENAME_CLIENT, {
        label: this.i18n.transform('Rename Device'),
        data: peer.name,
      })
      .subscribe(name => {
        this.service.renamePeer(profile, publicKey, name)
      })
  }

  async viewConfig(peer: VpnServerPeer) {
    const profile = this.server()?.profile
    if (!profile || !peer.public_key) return

    const config = await this.service.peerConfig(profile, peer.public_key)
    if (config) this.showConfig(peer.name, config, true)
  }

  reissue(peer: VpnServerPeer) {
    const profile = this.server()?.profile
    const publicKey = peer.public_key
    if (!profile || !publicKey) return

    this.dialogs
      .open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Reissue config?'),
        data: {
          content: this.i18n.transform(
            "This replaces the device's keys and address. Its current configuration stops working until you import the new one on the device.",
          ),
          yes: this.i18n.transform('Reissue'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.reissueAndShowConfig(profile, publicKey, peer.name))
  }

  toggleDnsInjection(peer: VpnServerPeer) {
    const profile = this.server()?.profile
    if (!profile || !peer.public_key) return
    const publicKey = peer.public_key

    if (peer.allow_dns_injection) {
      this.service.setPeerDnsInjection(profile, publicKey, false)
      return
    }

    this.dialogs
      .open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Allow DNS Record Publishing?'),
        data: {
          content: this.i18n.transform(
            'This device will be able to publish DNS names that resolve on your whole network. Grant this only to a device you trust, such as your own StartOS server.',
          ),
          yes: this.i18n.transform('Allow'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.service.setPeerDnsInjection(profile, publicKey, true),
      )
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

  private async addAndShowConfig(profile: string, peer: VpnServerPeer) {
    const response = await this.service.addPeer(profile, peer)
    if (response) {
      this.showConfig(peer.name, response.client_config, !!peer.public_key)
    }
  }

  private async reissueAndShowConfig(
    profile: string,
    publicKey: string,
    name: string,
  ) {
    const response = await this.service.reissuePeer(profile, publicKey)
    if (response) this.showConfig(name, response.client_config, false)
  }

  private showConfig(name: string, config: string, placeholderKey: boolean) {
    this.dialogs
      .open(CLIENT_CONFIG, {
        data: { name, config, placeholderKey } satisfies ClientConfigData,
      })
      .subscribe()
  }
}
