import { KeyValuePipe } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { getPkgId, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiCell } from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import { map } from 'rxjs'
import { ActionService } from 'src/app/services/action.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import {
  ALLOWED_STATUSES,
  BaseStatus,
  getInstalledBaseStatus,
  INACTIVE_STATUSES,
} from 'src/app/services/pkg-status-rendering.service'
import { StandardActionsService } from 'src/app/services/standard-actions.service'
import { getManifest } from 'src/app/utils/get-package-data'
import { ServiceActionComponent } from '../components/action.component'

@Component({
  template: `
    @if (package(); as pkg) {
      @let inactive = isInactive();

      @for (group of pkg.actions | keyvalue; track $index) {
        <section class="g-card">
          <header>{{ group.key }}</header>
          @for (a of group.value; track $index) {
            <button
              tuiCell
              [action]="a"
              [inactive]="inactive"
              (click)="handle(pkg.status, pkg.manifest, a)"
            ></button>
          }
        </section>
      }

      <section class="g-card">
        <header>StartOS</header>
        <button
          tuiCell
          [action]="rebuild"
          [inactive]="inactive"
          (click)="service.rebuild(pkg.manifest.id)"
        ></button>
        <button
          tuiCell
          [action]="uninstall"
          [inactive]="inactive"
          (click)="service.uninstall(pkg.manifest)"
        ></button>
      </section>
    }
  `,
  styles: `
    section {
      max-width: 42rem;
      display: flex;
      flex-direction: column;
      margin-bottom: 2rem;
    }
  `,
  host: { class: 'g-subpage' },
  imports: [ServiceActionComponent, TuiCell, KeyValuePipe],
})
export default class ServiceActionsRoute {
  private readonly actions = inject(ActionService)
  private readonly i18n = inject(i18nPipe)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  ungrouped: 'General' | 'Other' = 'General'

  readonly service = inject(StandardActionsService)

  readonly package = toSignal(
    this.patch.watch$('packageData', getPkgId()).pipe(
      map(pkg => {
        const specialGroup = Object.values(pkg.actions).some(a => !!a.group)
          ? 'Other'
          : 'General'
        const status = getInstalledBaseStatus(pkg.statusInfo)
        return {
          status,
          manifest: getManifest(pkg),
          actions: Object.entries(pkg.actions)
            .filter(([_, action]) => action.visibility !== 'hidden')
            .map(([id, action]) => ({
              ...action,
              id,
              group: action.group || specialGroup,
              visibility: ALLOWED_STATUSES[action.allowedStatuses].has(status)
                ? action.visibility
                : ({
                    disabled: `${this.i18n.transform('Action can only be executed when service is')} ${this.i18n.transform(action.allowedStatuses === 'only-running' ? 'Running' : 'Stopped')?.toLowerCase()}`,
                  } as T.ActionVisibility),
            }))
            .sort((a, b) => {
              if (a.group === specialGroup && b.group !== specialGroup) return 1
              if (b.group === specialGroup && a.group !== specialGroup)
                return -1

              const groupCompare = a.group.localeCompare(b.group)
              if (groupCompare !== 0) return groupCompare

              return a.id.localeCompare(b.id)
            })
            .reduce<
              Record<
                string,
                Array<T.ActionMetadata & { id: string; group: string }>
              >
            >((acc, action) => {
              const key = action.group
              if (!acc[key]) {
                acc[key] = []
              }
              acc[key].push(action)
              return acc
            }, {}),
        }
      }),
    ),
  )

  readonly rebuild = {
    name: this.i18n.transform('Rebuild Service')!,
    description: this.i18n.transform(
      'Rebuilds the service container. Only necessary if there is a bug in StartOS',
    )!,
  }

  readonly uninstall = {
    name: this.i18n.transform('Uninstall')!,
    description: this.i18n.transform(
      'Uninstalls this service from StartOS and deletes all data permanently.',
    )!,
  }

  handle(
    status: BaseStatus,
    { id, title }: T.Manifest,
    action: T.ActionMetadata & { id: string },
  ) {
    this.actions.present({
      pkgInfo: { id, title, status },
      actionInfo: { id: action.id, metadata: action },
    })
  }

  protected readonly isInactive = computed(
    (pkg = this.package()) => !pkg || INACTIVE_STATUSES.includes(pkg.status),
  )
}
