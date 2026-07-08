import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { TaskService } from '@start9labs/shared'
import { T, utils } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiComparator, TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiSkeleton } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { filter, map } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
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
        <h3 tuiTitle>{{ 'Subnets' | i18n }}</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd()">
            {{ 'Add' | i18n }}
          </button>
        </aside>
      </header>
      <table tuiTable class="g-table" [tuiSkeleton]="!subnets()">
        <thead>
          <tr>
            <th tuiTh [sorter]="byName">{{ 'Name' | i18n }}</th>
            <th tuiTh [sorter]="byRange">{{ 'IPv4 Range' | i18n }}</th>
            <th tuiTh>{{ 'DNS' | i18n }}</th>
            <th tuiTh>{{ 'WAN IPv4' | i18n }}</th>
            <th tuiTh>{{ 'IPv6 Prefix' | i18n }}</th>
            <th tuiTh></th>
          </tr>
        </thead>
        <tbody>
          @for (subnet of subnets() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>{{ subnet.name }}</td>
              <td tuiTd>{{ subnet.range }}</td>
              <td tuiTd>
                @switch (subnet.dns.type) {
                  @case ('device') {
                    {{ subnet.dnsDevice }}
                  }
                  @case ('custom') {
                    {{ 'custom' | i18n }}
                  }
                  @default {
                    {{ 'default' | i18n }}
                  }
                }
              </td>
              <td tuiTd>
                {{
                  wanLabel(subnet.wanIp, 'System default' | i18n, defaultWan())
                }}
              </td>
              <td tuiTd>{{ subnet.ipv6 ?? '—' }}</td>
              <td tuiTd [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAuto
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
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
                      (click)="onEdit(subnet)"
                    >
                      {{ 'Edit' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete(subnet.range)"
                    >
                      {{ 'Delete' | i18n }}
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="6">
                <app-placeholder icon="@tui.network">
                  {{ 'No subnets' | i18n }}
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
    TuiTable,
    i18nPipe,
  ],
})
export default class Subnets {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)
  private readonly i18n = inject(i18nPipe)

  protected readonly wanLabel = wanLabel

  protected readonly byName: TuiComparator<MappedSubnet> = (a, b) =>
    (a.name || '').localeCompare(b.name || '')

  protected readonly byRange: TuiComparator<MappedSubnet> = (a, b) =>
    this.ip4(a.range) - this.ip4(b.range)

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
          // Device name is data-derived (resolved here); the 'custom'/'default'
          // labels are translated in the template so they react to a live
          // language switch rather than being baked at data-emit time.
          dnsDevice:
            info.dns.type === 'device'
              ? (info.clients[info.dns.ip]?.name ?? info.dns.ip)
              : '',
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
        label: this.i18n.transform('Add Subnet'),
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
        label: this.i18n.transform('Edit Subnet'),
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

  protected onDelete(range: string): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () => await this.api.deleteSubnet({ subnet: range }),
        ),
      )
  }

  private ip4(s: string): number {
    return (s.split('/')[0] || '')
      .split('.')
      .reduce((n, o) => n * 256 + Number(o), 0)
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
  dnsDevice: string
  wanIp: string | null
  ipv6: string | null
}
