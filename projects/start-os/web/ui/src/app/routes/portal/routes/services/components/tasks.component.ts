import { Component, computed, input } from '@angular/core'
import { TuiTable } from '@taiga-ui/addon-table'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getLiveTasks } from 'src/app/utils/get-package-data'
import { ServiceTaskComponent } from './task.component'
import { i18nPipe } from '@start9labs/shared'

@Component({
  selector: 'service-tasks',
  template: `
    <header>{{ 'Tasks' | i18n }}</header>
    <table tuiTable class="g-table-service">
      <thead>
        <tr>
          <th tuiTh>{{ 'Action' }}</th>
          <th tuiTh>{{ 'Severity' }}</th>
          <th tuiTh>{{ 'Reason' | i18n }}</th>
          <th tuiTh></th>
        </tr>
      </thead>
      <tbody>
        @for (item of tasks(); track $index) {
          <tr [task]="item.task" [services]="services()"></tr>
        } @empty {
          <tr>
            <td colspan="4">
              <app-placeholder icon="@tui.list-checks">
                {{ 'All tasks complete' | i18n }}
              </app-placeholder>
            </td>
          </tr>
        }
      </tbody>
    </table>
  `,
  styles: `
    :host {
      min-height: 12rem;
      grid-column: span 7;
    }
  `,
  host: { class: 'g-card' },
  imports: [TuiTable, ServiceTaskComponent, PlaceholderComponent, i18nPipe],
})
export class ServiceTasksComponent {
  readonly pkg = input.required<PackageDataEntry>()
  readonly services = input.required<Record<string, PackageDataEntry>>()

  readonly tasks = computed(() =>
    getLiveTasks(this.pkg())
      .map(([replayId, entry]) => ({
        ...entry,
        task: { ...entry.task, replayId },
      }))
      .filter(t => t.active)
      .sort((a, b) => a.task.severity.localeCompare(b.task.severity)),
  )
}
