import { Component, inject, input, output } from '@angular/core'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import { TuiButton, TuiIcon } from '@taiga-ui/core'
import { Placeholder } from 'src/app/components/placeholder'
import { WwanNetwork, WwanSecurity } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { securityLabel } from './utils'

@Component({
  selector: '[wwanNetworks]',
  template: `
    <thead tuiThead>
      <tr>
        <th tuiTh>{{ 'Network name' | i18n }}</th>
        <th tuiTh [style.width.rem]="7">{{ 'Signal' | i18n }}</th>
        <th tuiTh [style.width.rem]="9">{{ 'Security' | i18n }}</th>
        <th tuiTh [style.width.rem]="6">{{ 'Channel' | i18n }}</th>
        <th tuiTh [style.width.rem]="7"></th>
      </tr>
    </thead>
    <tbody>
      @for (network of wwanNetworks(); track network.bssid || $index) {
        <tr>
          <td tuiTd>
            @if (network.ssid) {
              <strong>{{ network.ssid }}</strong>
            } @else {
              <span class="g-secondary">{{ 'Hidden network' | i18n }}</span>
            }
          </td>
          <td tuiTd>
            @if (network.signal !== null) {
              <tui-icon [icon]="signalIcon(network.signal)" />
              {{ network.signal }} dBm
            }
          </td>
          <td tuiTd>
            @if (network.security; as security) {
              {{ label(security) }}
            } @else {
              <span class="g-secondary">{{ 'Unsupported' | i18n }}</span>
            }
          </td>
          <td tuiTd>{{ network.channel }}</td>
          <td tuiTd>
            <button
              tuiButton
              size="xs"
              appearance="secondary"
              [disabled]="!network.security"
              (click)="join.emit(network)"
            >
              {{ 'Join' | i18n }}
            </button>
          </td>
        </tr>
      } @empty {
        <tr>
          <td tuiTd colspan="5">
            <app-placeholder icon="@tui.wifi-off">
              {{ 'No networks found' | i18n }}
            </app-placeholder>
          </td>
        </tr>
      }
    </tbody>
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [TuiTable, TuiButton, TuiIcon, Placeholder, i18nPipe],
})
export class WwanNetworksTable {
  private readonly i18n = inject(i18nPipe)

  protected readonly label = (security: WwanSecurity): string =>
    securityLabel(this.i18n, security)

  public readonly wwanNetworks = input<WwanNetwork[]>([])
  public readonly join = output<WwanNetwork>()

  protected signalIcon(dbm: number): string {
    if (dbm >= -60) return '@tui.wifi-high'
    return dbm >= -72 ? '@tui.wifi' : '@tui.wifi-low'
  }
}
