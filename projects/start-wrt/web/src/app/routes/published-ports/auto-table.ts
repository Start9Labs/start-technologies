import { Component, inject, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import { TuiLink } from '@taiga-ui/core'
import { AutoForwardDisplay } from './types'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

/**
 * Read-only table of forwards trusted devices opened for themselves via
 * PCP/UPnP. There are no actions: the device renews or withdraws its own
 * forwards, and an unrenewed forward expires on its own. To stop a device
 * creating them, turn off its toggle on the device page.
 */
@Component({
  selector: '[autoForwards]',
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
        <th tuiTh [sorter]="'label' | tuiSorter" [style.min-width.rem]="8">
          {{ 'Protocol' | i18n }}
        </th>
        <th tuiTh [style.min-width.rem]="7">{{ 'Expires' | i18n }}</th>
      </tr>
    </thead>
    <tbody>
      @for (item of autoForwards() | tuiTableSort; track item.id) {
        <tr>
          <td tuiTd>
            <a
              tuiLink
              routerLink="/devices/device"
              [queryParams]="{ mac: item.deviceMac }"
              [state]="{ returnUrl: '/published-ports' }"
            >
              {{ item.deviceName || item.deviceMac }}
            </a>
          </td>
          <td tuiTd>{{ item.ports }}</td>
          <td tuiTd>{{ item.publicPorts }}</td>
          <td tuiTd>{{ item.label }}</td>
          <td tuiTd>{{ expiry(item) }}</td>
        </tr>
      }
    </tbody>
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [RouterLink, TuiTable, TuiLink, i18nPipe],
})
export class AutoForwardsTable {
  public readonly autoForwards = input<AutoForwardDisplay[]>([])

  private readonly i18n = inject(i18nPipe)

  protected expiry(item: AutoForwardDisplay): string {
    return item.expiresSecs === undefined
      ? '—'
      : `${Math.max(1, Math.round(item.expiresSecs / 60))} ${this.i18n.transform('min')}`
  }
}
