import { Component, inject } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { GatewaysItemComponent } from './item.component'
import { GatewayService } from 'src/app/services/gateway.service'

@Component({
  selector: 'gateways-table',
  template: `
    <table [appTable]="['Name', 'Type', $any('LAN IP'), $any('WAN IP'), null]">
      @for (gateway of gatewayService.gateways(); track $index) {
        <tr [gateway]="gateway"></tr>
      } @empty {
        <tr>
          <td colspan="6">
            <div [tuiSkeleton]="true">{{ 'Loading' | i18n }}</div>
          </td>
        </tr>
      }
    </table>
  `,
  imports: [TuiSkeleton, i18nPipe, TableComponent, GatewaysItemComponent],
})
export class GatewaysTableComponent {
  protected readonly gatewayService = inject(GatewayService)
}
