import { Component, input } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiComparator,
  TuiTable,
  TuiTableDirective,
} from '@taiga-ui/addon-table'

@Component({
  selector: 'table[appTable]',
  template: `
    <thead>
      <tr>
        <ng-content select="th" />
        @if (!appTableSelected()) {
          @for (header of appTable(); track $index) {
            <th
              tuiTh
              [requiredSort]="true"
              [sorter]="appTableSorters()[$index] || null"
            >
              {{ header | i18n }}
            </th>
          }
        }
      </tr>
    </thead>
    <tbody><ng-content /></tbody>
    <ng-content select="tbody" />
    <ng-content select="caption" />
  `,
  styles: `
    thead {
      display: var(--app-table-header-display, table-header-group);
    }

    :host:has(app-placeholder) thead {
      display: none;
    }
  `,
  host: { class: 'g-table' },
  hostDirectives: [
    {
      directive: TuiTableDirective,
      inputs: ['sorter'],
    },
  ],
  imports: [i18nPipe, TuiTable],
})
export class TableComponent {
  readonly appTable = input.required<ReadonlyArray<i18nKey | null>>()
  readonly appTableSorters = input<ReadonlyArray<TuiComparator<any> | null>>([])
  // Number of currently-selected rows. When > 0 the column headers are hidden
  // so the projected header cell can show the group action + count instead.
  readonly appTableSelected = input(0)
}
