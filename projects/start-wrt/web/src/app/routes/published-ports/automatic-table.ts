import { Component, inject, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import { TuiLink } from '@taiga-ui/core'
import { AutomaticPortUseDisplay } from './types'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[automaticPortUses]',
  template: `
    <thead tuiThead>
      <tr>
        <th
          tuiTh
          [sorter]="'deviceName' | tuiSorter"
          [style.min-width.rem]="10"
        >
          {{ 'Device' | i18n }}
        </th>
        <th tuiTh [sorter]="'ports' | tuiSorter" [style.min-width.rem]="6">
          {{ 'Port' | i18n }}
        </th>
        <th
          tuiTh
          [sorter]="'publicPorts' | tuiSorter"
          [style.min-width.rem]="8"
        >
          {{ 'Public port' | i18n }}
        </th>
        <th tuiTh [sorter]="'hostname' | tuiSorter" [style.min-width.rem]="10">
          {{ 'Hostname' | i18n }}
        </th>
        <th tuiTh [sorter]="'kind' | tuiSorter" [style.min-width.rem]="8">
          {{ 'Kind' | i18n }}
        </th>
        <th tuiTh [style.min-width.rem]="7">{{ 'Expires' | i18n }}</th>
      </tr>
    </thead>
    <tbody>
      @for (item of automaticPortUses() | tuiTableSort; track item.id) {
        <tr>
          <td tuiTd>
            @if (item.deviceMac) {
              <a
                tuiLink
                routerLink="/devices/device"
                [queryParams]="{ mac: item.deviceMac }"
                [state]="{ returnUrl: '/published-ports' }"
              >
                {{ item.deviceName || item.deviceMac }}
              </a>
            } @else {
              {{ 'Unknown' | i18n }}
            }
          </td>
          <td tuiTd>{{ item.ports }}</td>
          <td tuiTd>{{ item.publicPorts }}</td>
          <td tuiTd>{{ item.hostname || '—' }}</td>
          <td tuiTd>{{ item.kind }}</td>
          <td tuiTd>{{ expiry(item) }}</td>
        </tr>
      }
    </tbody>
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [RouterLink, TuiTable, TuiLink, i18nPipe],
})
export class AutomaticPortUsesTable {
  public readonly automaticPortUses = input<AutomaticPortUseDisplay[]>([])

  private readonly i18n = inject(i18nPipe)

  protected expiry(item: AutomaticPortUseDisplay): string {
    return item.expiresSecs === undefined
      ? '—'
      : `${Math.max(1, Math.round(item.expiresSecs / 60))} ${this.i18n.transform('min')}`
  }
}
