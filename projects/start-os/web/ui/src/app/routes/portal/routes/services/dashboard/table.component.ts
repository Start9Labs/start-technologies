import { Component, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiTable } from '@taiga-ui/addon-table'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { DepErrorService } from 'src/app/services/dep-error.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { ToManifestPipe } from '../../../pipes/to-manifest'
import { ServiceComponent } from './service.component'
import { byName, byStatus } from './sorters'

@Component({
  selector: '[services]',
  template: `
    <table
      [appTable]="[null, 'Service', 'Status', 'Version', 'Uptime']"
      [appTableSorters]="[null, byName, byStatus]"
      [sorter]="byName"
    >
      @for (service of services() | tuiTableSort; track $index) {
        <tr
          appService
          [depErrors]="errors()?.[(service | toManifest).id] || {}"
          [pkg]="service"
          [routerLink]="'/services/' + (service | toManifest)?.id"
        ></tr>
      } @empty {
        @for (_ of ['', '']; track $index) {
          <tr>
            <td colspan="5">
              <div [tuiSkeleton]="true">{{ 'Loading' | i18n }}</div>
            </td>
          </tr>
        }
      }
    </table>
  `,
  imports: [
    FormsModule,
    RouterLink,
    ServiceComponent,
    TableComponent,
    ToManifestPipe,
    TuiSkeleton,
    TuiTable,
    i18nPipe,
  ],
})
export class ServicesTableComponent {
  private readonly depErrors = inject(DepErrorService)

  readonly services = input.required<readonly PackageDataEntry[] | null>()

  protected readonly errors = toSignal(this.depErrors.depErrors$)
  protected readonly byName = byName
  protected readonly byStatus = byStatus
}
