import { Component, computed, inject, input } from '@angular/core'
import { Router } from '@angular/router'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter, firstValueFrom } from 'rxjs'

import { ServiceTasksComponent } from 'src/app/routes/portal/routes/services/components/tasks.component'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import {
  ALLOWED_STATUSES,
  getInstalledBaseStatus,
  INACTIVE_STATUSES,
} from 'src/app/services/pkg-status-rendering.service'
import { getManifest } from 'src/app/utils/get-package-data'

type BackupReviewDecision = 'add' | 'create'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Add to backup schedule' | i18n }}</h2>
    </header>
    <footer>
      <button
        tuiButton
        appearance="primary"
        (click)="context.completeWith('add')"
      >
        {{ 'Add to current schedule' | i18n }}
      </button>
      <button
        tuiButton
        appearance="flat"
        (click)="context.completeWith('create')"
      >
        {{ 'Create a new schedule' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: grid;
      gap: 1.25rem;
    }

    footer {
      display: flex;
      flex-wrap: wrap;
      justify-content: flex-end;
      gap: 0.75rem;
    }
  `,
  imports: [TuiButton, TuiHeader, TuiTitle, i18nPipe],
})
class BackupReviewDialog {
  protected readonly context =
    injectContext<TuiDialogContext<BackupReviewDecision, void>>()
}

const BACKUP_REVIEW_DIALOG = new PolymorpheusComponent(BackupReviewDialog)

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
        <img [src]="pkg()?.icon || fallback()?.icon" alt="" />
      </i>
      <strong>
        @if (backupReview()) {
          {{ 'Add to backup schedule' | i18n }}
        } @else {
          {{
            pkg()?.actions?.[task().actionId]?.name || ('Not installed' | i18n)
          }}
        }
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
      max-inline-size: 15rem;
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
        max-inline-size: none;
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
  imports: [TuiButton, TuiAvatar, i18nPipe, TuiFade],
})
export class ServiceTaskComponent {
  private readonly actionService = inject(ActionService)
  private readonly dialog = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly tasks = inject(TaskService)
  private readonly component = inject(ServiceTasksComponent)
  private readonly i18n = inject(i18nPipe)

  readonly task = input.required<T.Task & { replayId: string }>()
  readonly services = input.required<Record<string, PackageDataEntry>>()

  protected readonly pkg = computed(
    () => this.services()[this.task().packageId],
  )
  protected readonly backupReview = computed(
    () => this.task().actionId === 'add-to-backup-schedule',
  )
  protected readonly title = computed(
    (pkg = this.pkg()) => pkg && getManifest(pkg).title,
  )

  protected readonly fallback = computed(
    () => this.component.pkg().currentDependencies[this.task().packageId],
  )

  protected readonly disabled = computed(() => {
    if (this.backupReview()) return false

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

  protected async dismiss() {
    const { packageId, replayId } = this.task()

    this.dialog
      .openConfirm(DISMISS)
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(async () => {
          if (!this.backupReview()) {
            await this.api.clearTask({ packageId, replayId, force: false })
            return
          }

          const [jobs, reviews] = await Promise.all([
            this.api.getScheduledBackupJobs({}),
            this.api.getNewServiceBackupReviews({}),
          ])
          const review = reviews.find(item => item.packageId === packageId)
          if (!review) {
            await this.api.clearTask({ packageId, replayId, force: false })
            return
          }
          await this.api.resolveNewServiceBackupReview({
            packageId,
            decisions: Object.fromEntries(jobs.map(job => [job.id, false])),
          })
        }),
      )
  }

  protected async handle() {
    const task = this.task()
    if (this.backupReview()) {
      let jobs: T.BackupJob[] = []
      let reviews: T.NewServiceBackupReview[] = []
      const loaded = await this.tasks.run(async () => {
        ;[jobs, reviews] = await Promise.all([
          this.api.getScheduledBackupJobs({}),
          this.api.getNewServiceBackupReviews({}),
        ])
      }, 'Loading')
      if (!loaded) return
      const review = reviews.find(item => item.packageId === task.packageId)
      if (!review) return
      if (jobs.length === 1) {
        const decision = await firstValueFrom(
          this.dialog.openComponent<BackupReviewDecision>(
            BACKUP_REVIEW_DIALOG,
            {
              size: 's',
            },
          ),
          { defaultValue: null },
        )
        if (decision === 'add') {
          await this.tasks.run(async () => {
            await this.api.resolveNewServiceBackupReview({
              packageId: task.packageId,
              decisions: Object.fromEntries(
                jobs.map(job => [job.id, job.id === jobs[0]!.id]),
              ),
            })
          })
        } else if (decision === 'create') {
          await this.router.navigate(['/system/backups'], {
            queryParams: {
              addService: task.packageId,
              createSchedule: true,
            },
          })
        }
        return
      }
      await this.router.navigate(['/system/backups'], {
        queryParams: { addService: task.packageId },
      })
      return
    }

    const title = this.title()
    const pkg = this.pkg()
    const metadata = pkg?.actions[task.actionId]

    if (title && pkg && metadata) {
      this.actionService.present({
        pkgInfo: {
          id: task.packageId,
          title,
          status: getInstalledBaseStatus(pkg.statusInfo),
          icon: pkg.icon,
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
