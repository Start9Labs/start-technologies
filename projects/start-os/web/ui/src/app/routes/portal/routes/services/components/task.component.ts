import { AsyncPipe } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { filter } from 'rxjs'
import { ServiceTasksComponent } from 'src/app/routes/portal/routes/services/components/tasks.component'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import {
  ALLOWED_STATUSES,
  getInstalledBaseStatus,
  INACTIVE_STATUSES,
} from 'src/app/services/pkg-status-rendering.service'
import { getManifest } from 'src/app/utils/get-package-data'

@Component({
  selector: 'tr[task]',
  template: `
    <td tuiFade class="name">
      <i
        tuiAvatar
        appearance="action-grayscale"
        size="xs"
        [round]="false"
        [title]="title() || fallback()?.title"
      >
        <img [src]="icon() | async" alt="" />
      </i>
      <strong>
        {{
          pkg()?.actions?.[task().actionId]?.name || ('Not installed' | i18n)
        }}
      </strong>
    </td>
    <td class="severity">
      @if (task().severity === 'critical') {
        <strong class="g-warning">{{ 'Required' | i18n }}</strong>
      } @else if (task().severity === 'important') {
        <strong class="g-info">{{ 'Recommended' | i18n }}</strong>
      } @else {
        <strong>{{ 'Optional' | i18n }}</strong>
      }
    </td>
    <td class="reason g-secondary">
      {{ task().reason || ('No reason provided' | i18n) }}
      @if (disabled()) {
        <div class="g-warning">{{ disabled() }}</div>
      }
    </td>
    <td class="actions">
      @if (task().severity !== 'critical') {
        <button
          tuiIconButton
          iconStart="@tui.trash"
          appearance="primary-destructive"
          [disabled]="!pkg()"
          (click)="dismiss()"
        >
          {{ 'Dismiss' | i18n }}
        </button>
      }
      <button
        tuiIconButton
        iconStart="@tui.play"
        appearance="primary-success"
        [disabled]="!!disabled()"
        (click)="handle()"
      >
        {{ 'Run' | i18n }}
      </button>
    </td>
  `,
  styles: `
    .name {
      white-space: nowrap;
      max-width: 15rem;
      overflow: hidden;
    }

    .name strong {
      margin-inline-start: 0.5rem;
      line-height: 1.5rem;
      vertical-align: middle;
    }

    td:not(:last-child) {
      padding-inline-end: 1.5rem;
    }

    .actions {
      white-space: nowrap;
      justify-content: end;
      display: flex;
      gap: 8px;
    }

    :host-context(tui-root._mobile) {
      display: grid;
      grid-template-columns: 1fr min-content;
      align-items: center;
      gap: 0.5rem 1rem;
      padding: 1rem 0.5rem;

      td {
        display: flex;
        align-items: center;
        padding: 0;
      }

      .name {
        grid-area: 1 / 1;
        max-width: none;
        white-space: normal;
        overflow: visible;
      }

      .severity {
        grid-area: 2 / 1;
      }

      .reason {
        grid-area: 3 / 1;
      }

      .actions {
        grid-area: 1 / 2 / 4 / 3;
      }
    }
  `,
  host: {
    '[style.opacity]': '!disabled() ? null : "var(--tui-disabled-opacity)"',
  },
  imports: [TuiButton, TuiAvatar, i18nPipe, TuiFade, AsyncPipe],
})
export class ServiceTaskComponent {
  private readonly actionService = inject(ActionService)
  private readonly dialog = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly component = inject(ServiceTasksComponent)
  private readonly i18n = inject(i18nPipe)
  private readonly icons = inject(PackageIconService)

  readonly task = input.required<T.Task & { replayId: string }>()
  readonly services = input.required<Record<string, PackageDataEntry>>()

  readonly pkg = computed(() => this.services()[this.task().packageId])
  readonly title = computed((pkg = this.pkg()) => pkg && getManifest(pkg).title)

  readonly icon = computed((pkg = this.pkg()) =>
    pkg
      ? this.icons.get(pkg)
      : this.icons.get(this.component.pkg(), this.task().packageId),
  )

  readonly fallback = computed(
    () => this.component.pkg().currentDependencies[this.task().packageId],
  )

  readonly disabled = computed(() => {
    const pkg = this.pkg()
    if (!pkg) return this.i18n.transform('Not installed')!

    const action = pkg.actions[this.task().actionId]
    if (!action) return this.i18n.transform('Action not found')!

    const status = getInstalledBaseStatus(pkg.statusInfo)

    if (INACTIVE_STATUSES.includes(status)) return status as string

    if (!ALLOWED_STATUSES[action.allowedStatuses].has(status)) {
      return `${this.i18n.transform('Action can only be executed when service is')} ${this.i18n.transform(action.allowedStatuses === 'only-running' ? 'Running' : 'Stopped')?.toLowerCase()}`
    }

    if (typeof action.visibility === 'object') {
      return action.visibility.disabled
    }

    return false
  })

  async dismiss() {
    const { packageId, replayId } = this.task()

    this.dialog
      .openConfirm(DISMISS)
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () =>
            await this.api.clearTask({ packageId, replayId, force: false }),
        ),
      )
  }

  async handle() {
    const task = this.task()
    const title = this.title()
    const pkg = this.pkg()
    const metadata = pkg?.actions[task.actionId]

    if (title && pkg && metadata) {
      this.actionService.present({
        pkgInfo: {
          id: task.packageId,
          title,
          status: getInstalledBaseStatus(pkg.statusInfo),
        },
        actionInfo: { id: task.actionId, metadata },
        prefill: task.input?.set,
      })
    }
  }
}

const DISMISS = {
  label: 'Confirm',
  size: 's',
  data: {
    content: 'Are you sure you want to dismiss this task?',
    yes: 'Dismiss',
    no: 'Cancel',
  },
} as const
