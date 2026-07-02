import { Component, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiTable } from '@taiga-ui/addon-table'
import { TuiFormatNumberPipe, TuiLink } from '@taiga-ui/core'
import { TuiChip } from '@taiga-ui/kit'
import { Placeholder } from 'src/app/components/placeholder'
import { DeviceTableItem } from 'src/app/routes/devices/utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: 'table[devicesOnline]',
  template: `
    <caption [style.background]="'var(--tui-status-positive-pale)'">
      {{ 'Online' | i18n }}
    </caption>
    <thead>
      <tr>
        <th tuiTh [sorter]="'name' | tuiSorter">{{ 'Name' | i18n }}</th>
        <th tuiTh>{{ 'Connection' | i18n }}</th>
        <th tuiTh>{{ 'Security Profile' | i18n }}</th>
        <th tuiTh>{{ 'MAC' | i18n }}</th>
        <th tuiTh>{{ 'IP' | i18n }}</th>
        <th tuiTh>{{ 'Data' | i18n }}</th>
        <th tuiTh [style.text-align]="'start'">{{ 'Speed' | i18n }}</th>
        <th tuiTh></th>
      </tr>
    </thead>
    <tbody>
      @for (
        item of devicesOnline() | tuiTableSort;
        track item.mac ?? item.ipv4
      ) {
        <tr>
          <td tuiTd>
            @if (item.mac) {
              <a tuiLink routerLink="device" [queryParams]="{ mac: item.mac }">
                <strong>{{ item.name }}</strong>
              </a>
            } @else {
              <strong>{{ item.name }}</strong>
            }
          </td>
          <td tuiTd>
            <div
              tuiChip
              size="xs"
              [iconStart]="getConnectionIcon(item.connection)"
            >
              {{ item.connection }}
            </div>
          </td>
          <td tuiTd>
            <a tuiLink routerLink="/profiles">
              {{ item.securityProfile || ('Default' | i18n) }}
            </a>
          </td>
          <td tuiTd>{{ item.mac || '-' }}</td>
          <td tuiTd>
            @if (item.ipv4) {
              <div
                tuiChip
                size="xs"
                [iconStart]="item.ipv4Static ? '@tui.lock' : ''"
              >
                {{ item.ipv4 }}
              </div>
            }
            @if (item.ipv6) {
              <div
                tuiChip
                size="xs"
                [iconStart]="item.ipv6Static ? '@tui.lock' : ''"
              >
                {{ item.ipv6 }}
              </div>
            }
          </td>
          <td tuiTd>
            {{ item.dataUsage ?? 0 | tuiFormatNumber }}
            <small class="g-secondary">GB</small>
          </td>
          <td tuiTd>
            @if (item.speed) {
              <div tuiChip size="xs" iconStart="@tui.arrow-up">
                <span>
                  {{ item.speed.up | tuiFormatNumber }}
                  <small class="g-secondary">MB/s</small>
                </span>
              </div>
              <div tuiChip size="xs" iconStart="@tui.arrow-down">
                <span>
                  {{ item.speed.down | tuiFormatNumber }}
                  <small class="g-secondary">MB/s</small>
                </span>
              </div>
            }
          </td>
          <td tuiTd></td>
        </tr>
      } @empty {
        <tr>
          <td colspan="8">
            <app-placeholder icon="@tui.monitor-smartphone">
              {{ 'No online devices' | i18n }}
            </app-placeholder>
          </td>
        </tr>
      }
    </tbody>
  `,
  imports: [
    RouterLink,
    TuiTable,
    TuiFormatNumberPipe,
    TuiChip,
    TuiLink,
    Placeholder,
    i18nPipe,
  ],
  host: { class: 'g-table' },
})
export class DevicesOnline {
  readonly devicesOnline = input<readonly DeviceTableItem[]>([])

  protected getConnectionIcon(connection?: string): string {
    if (!connection) return '@tui.monitor'
    const lower = connection.toLowerCase()
    if (lower.includes('ethernet') || lower.includes('eth')) {
      return '@tui.cable'
    }
    if (lower.includes('wi-fi') || lower.includes('wifi')) {
      return '@tui.wifi'
    }
    if (lower.startsWith('vpn')) {
      return '@tui.shield'
    }
    return '@tui.monitor'
  }
}
