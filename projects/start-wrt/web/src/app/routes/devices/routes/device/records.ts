import { Component, input } from '@angular/core'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import { InjectedDnsRecordFromApi } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

/** Read-only table of the DNS records this device published into the router. */
@Component({
  selector: '[injectedRecords]',
  template: `
    <thead tuiThead>
      <tr>
        <th tuiTh [style.min-width.rem]="12">{{ 'Name' | i18n }}</th>
        <th tuiTh [style.min-width.rem]="5">{{ 'Type' | i18n }}</th>
        <th tuiTh [style.min-width.rem]="10">{{ 'Resolves to' | i18n }}</th>
      </tr>
    </thead>
    <tbody>
      @for (
        item of injectedRecords();
        track item.name + item.rtype + item.value
      ) {
        <tr>
          <td tuiTd>{{ item.name }}</td>
          <td tuiTd>{{ item.rtype }}</td>
          <td tuiTd>{{ item.value }}</td>
        </tr>
      }
    </tbody>
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [TuiTable, i18nPipe],
})
export class InjectedRecordsTable {
  readonly injectedRecords = input<InjectedDnsRecordFromApi[]>([])
}
