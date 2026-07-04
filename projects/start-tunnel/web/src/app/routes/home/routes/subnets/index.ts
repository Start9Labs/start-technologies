import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { T, utils } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TUI_CONFIRM,
  TuiNotificationMiddleService,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { filter, map } from 'rxjs'
import { PlaceholderComponent } from 'src/app/routes/home/components/placeholder'
import {
  defaultWanIp,
  wanLabel,
  wanOptions,
} from 'src/app/routes/home/components/wan'
import { ApiService } from 'src/app/services/api/api.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'

import { SUBNETS_ADD } from './add'

@Component({
  template: `
    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.network" />
        <h3 tuiTitle>Subnets</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd()">Add</button>
        </aside>
      </header>
      <table class="g-table" [tuiSkeleton]="!subnets()">
        <thead>
          <tr>
            <th>Name</th>
            <th>IPv4 Range</th>
            <th>DNS</th>
            <th>WAN IPv4</th>
            <th>IPv6 Prefix</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          @for (subnet of subnets(); track $index) {
            <tr>
              <td>{{ subnet.name }}</td>
              <td>{{ subnet.range }}</td>
              <td>{{ subnet.dnsLabel }}</td>
              <td>
                {{ wanLabel(subnet.wanIp, 'System default', defaultWan()) }}
              </td>
              <td>{{ subnet.ipv6 ?? '—' }}</td>
              <td [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAuto
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
                >
                  Actions
                  <tui-data-list
                    *tuiDropdown="let close"
                    size="s"
                    (click)="close()"
                  >
                    <button
                      tuiOption
                      iconStart="@tui.pencil"
                      (click)="onEdit(subnet)"
                    >
                      Edit
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete($index)"
                    >
                      Delete
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="6">
                <app-placeholder icon="@tui.network">
                  No subnets
                </app-placeholder>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }
  `,
  imports: [
    TuiButton,
    TuiCardLarge,
    TuiDropdown,
    TuiDataList,
    PlaceholderComponent,
    TuiSkeleton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
  ],
})
export default class Subnets {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly api = inject(ApiService)
  private readonly loading = inject(TuiNotificationMiddleService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)

  protected readonly wanLabel = wanLabel

  private readonly wans = toSignal(
    this.patch.watch$('gateways').pipe(map(wanOptions)),
    { initialValue: [] },
  )

  protected readonly defaultWan = toSignal(
    this.patch.watch$('gateways').pipe(map(defaultWanIp)),
    { initialValue: null },
  )

  protected readonly subnets = toSignal(
    this.patch.watch$('wg', 'subnets').pipe(
      map(s =>
        Object.entries(s).map(([range, info]) => ({
          range,
          name: info.name,
          hasClients: !!Object.keys(info.clients).length,
          dns: info.dns,
          clients: info.clients,
          dnsLabel: dnsLabel(info.dns, info.clients),
          wanIp: info.wanIp,
          ipv6: info.ipv6,
        })),
      ),
    ),
    { initialValue: null },
  )

  protected onAdd(): void {
    this.dialogs
      .open(SUBNETS_ADD, {
        label: 'Add Subnet',
        data: {
          subnet: this.getNext(),
          mode: 'default',
          device: null,
          servers: [],
          devices: [],
          wanIp: null,
          wanOptions: this.wans(),
          defaultWan: this.defaultWan(),
          ipv6: null,
        },
      })
      .subscribe()
  }

  protected onEdit({
    range,
    name,
    dns,
    clients,
    wanIp,
    ipv6,
  }: MappedSubnet): void {
    const devices = Object.entries(clients).map(([ip, client]) => ({
      ip,
      name: client.name,
    }))

    this.dialogs
      .open(SUBNETS_ADD, {
        label: 'Edit Subnet',
        data: {
          subnet: range,
          name,
          mode: dns.type,
          device:
            dns.type === 'device'
              ? (devices.find(d => d.ip === dns.ip) ?? null)
              : null,
          servers: dns.type === 'custom' ? dns.servers : [],
          devices,
          wanIp,
          wanOptions: this.wans(),
          defaultWan: this.defaultWan(),
          ipv6,
        },
      })
      .subscribe()
  }

  protected onDelete(index: number): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: 'Are you sure?' })
      .pipe(filter(Boolean))
      .subscribe(async () => {
        const subnet = this.subnets()?.[index]?.range || ''
        const loader = this.loading.open('').subscribe()

        try {
          await this.api.deleteSubnet({ subnet })
        } catch (e) {
          console.log(e)
        } finally {
          loader.unsubscribe()
        }
      })
  }

  private getNext(): string {
    const current = this.subnets()?.map(s => utils.IpNet.parse(s.range))
    const suggestion = utils.IpNet.parse('10.59.0.1/24')

    for (let i = 0; i < 256; i++) {
      suggestion.octets[2] = Math.floor(Math.random() * 256)
      if (
        !current?.some(
          s => s.contains(suggestion), // inverse check unnecessary since we don't allow subnets smaller than /24
        )
      ) {
        return suggestion.ipnet
      }
    }

    // No recommendation if can't find a /24 from 10.59 in 256 random tries
    return ''
  }
}

type MappedSubnet = {
  range: string
  name: string
  hasClients: boolean
  dns: T.Tunnel.DnsConfig
  clients: T.Tunnel.WgSubnetClients
  dnsLabel: string
  wanIp: string | null
  ipv6: string | null
}

function dnsLabel(
  dns: T.Tunnel.DnsConfig,
  clients: T.Tunnel.WgSubnetClients,
): string {
  switch (dns.type) {
    case 'device':
      return clients[dns.ip]?.name ?? dns.ip
    case 'custom':
      return 'custom'
    default:
      return 'default'
  }
}
