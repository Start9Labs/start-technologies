import { Component, input } from '@angular/core'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import { InjectedDnsRecordFromApi } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

/**
 * Read-only table of DNS records this device published into the router.
 * There are no actions: the device re-asserts or withdraws its own records,
 * and the router drops them when the device loses the address they point at.
 * To stop a device publishing, turn off its toggle above.
 */
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
