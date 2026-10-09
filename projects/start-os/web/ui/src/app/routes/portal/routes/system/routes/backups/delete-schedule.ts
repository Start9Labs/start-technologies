import { Component, inject, Service } from '@angular/core'
import { FormsModule } from '@angular/forms'
import {
  convertBytes,
  DialogService,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCheckbox,
  TuiDialogContext,
  TuiTitle,
} from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { TuiHeader } from '@taiga-ui/layout'
import { firstValueFrom } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'

import { BackupService } from './backup.service'

export interface DeleteScheduleDialogData {
  checkpointCount: number
  reclaimable: string
}

export interface DeleteScheduleDecision {
  deleteCheckpoints: boolean
}

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ 'Delete backup schedule?' | i18n }}
      </h2>
    </header>
    <p>
      {{
        'Snapshots that are no longer referenced will be kept as an archive by default.'
          | i18n
      }}
    </p>

    <label #deleteOption class="delete-option">
      <input tuiCheckbox type="checkbox" [(ngModel)]="deleteCheckpoints" />
      <span tuiTitle>
        <b>{{ 'Delete related backups' | i18n }}</b>
        <span tuiSubtitle>
          {{ context.data.checkpointCount }} {{ 'Checkpoints' | i18n }} ·
          {{ context.data.reclaimable }}
        </span>
      </span>
    </label>

    <footer class="actions">
      <button tuiButton size="s" appearance="primary" (click)="cancel()">
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        size="s"
        appearance="primary-destructive"
        (click)="confirm(deleteOption)"
      >
        <span class="delete-only">{{ 'Delete schedule' | i18n }}</span>
        <span class="delete-with-backups">
          {{ 'Delete schedule and backups' | i18n }}
        </span>
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: grid;
      gap: 1.25rem;
      min-inline-size: 0;
    }

    p {
      margin: 0;
      color: var(--tui-text-secondary);
    }

    .delete-option {
      display: flex;
      align-items: flex-start;
      gap: 0.75rem;
      min-inline-size: 0;
      cursor: pointer;
    }

    [tuiTitle] {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    [tuiSubtitle] {
      display: block;
      margin-block-start: 0.25rem;
    }

    .actions {
      display: flex;
      flex-wrap: wrap;
      justify-content: flex-end;
      gap: 0.75rem;
    }

    .actions > button {
      max-inline-size: 100%;
      min-inline-size: 0;
      min-block-size: 2.75rem;
      block-size: auto;
      padding-block: 0.5rem;
      white-space: normal;
    }

    .delete-only,
    .delete-with-backups {
      inline-size: 100%;
      min-inline-size: 0;
      overflow-wrap: anywhere;
      text-align: center;
      white-space: normal;
    }

    :host-context(tui-root._mobile) .actions {
      align-items: stretch;
      flex-direction: column;
    }

    :host-context(tui-root._mobile) .actions > button {
      inline-size: 100%;
    }

    .delete-with-backups,
    :host:has(.delete-option input:checked) .delete-only {
      display: none;
    }

    :host:has(.delete-option input:checked) .delete-with-backups {
      display: inline;
    }
  `,
  imports: [FormsModule, TuiButton, TuiCheckbox, TuiHeader, TuiTitle, i18nPipe],
})
export class DeleteScheduleDialog {
  protected readonly context =
    injectContext<
      TuiDialogContext<DeleteScheduleDecision | null, DeleteScheduleDialogData>
    >()

  protected deleteCheckpoints = false

  protected cancel() {
    this.context.completeWith(null)
  }

  protected confirm(deleteOption: HTMLLabelElement) {
    this.context.completeWith({
      deleteCheckpoints: deleteOption.querySelector('input')?.checked ?? false,
    })
  }
}

export const DELETE_SCHEDULE_DIALOG = new PolymorpheusComponent(
  DeleteScheduleDialog,
)

@Service()
export class DeleteScheduleService {
  private readonly api = inject(ApiService)
  private readonly backupService = inject(BackupService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)

  async delete(job: T.BackupJob): Promise<boolean> {
    let histories: T.ServiceTargetHistory[] = []
    const loaded = await this.tasks.run(async () => {
      histories = await this.api.getScheduledBackupHistories({})
    }, 'Loading')
    if (!loaded) return false

    const unreferenced = this.unreferencedHistories(histories, job)
    const checkpointCount = unreferenced.reduce(
      (sum, history) => sum + history.snapshots.length,
      0,
    )
    const reclaimable = unreferenced.reduce(
      (sum, history) => sum + this.historyBytes(history),
      0,
    )
    const decision = await firstValueFrom(
      this.dialogs.openComponent<DeleteScheduleDecision | null>(
        DELETE_SCHEDULE_DIALOG,
        {
          size: 's',
          data: {
            checkpointCount,
            reclaimable: convertBytes(reclaimable),
          },
        },
      ),
      { defaultValue: null },
    )
    if (!decision) return false

    const password = decision.deleteCheckpoints
      ? await firstValueFrom(
          this.dialogs.openPrompt<string>({
            label: 'Master password needed',
            data: {
              message: 'Enter master password',
              label: 'Password',
              placeholder: 'Enter master password',
              buttonText: 'Delete schedule and backups',
              useMask: true,
            },
          }),
          { defaultValue: '' },
        )
      : ''
    if (decision.deleteCheckpoints && !password) return false

    let deleted = false
    const completed = await this.tasks.run(
      async () => {
        if (decision.deleteCheckpoints) {
          deleted =
            (await this.backupService.withOriginalPassword(
              async oldPassword => {
                await this.api.deleteScheduledBackupJobWithBackups({
                  id: job.id,
                  password,
                  oldPassword,
                })
                return true
              },
            )) ?? false
        } else {
          await this.api.deleteScheduledBackupJob({ id: job.id })
          deleted = true
        }
      },
      decision.deleteCheckpoints
        ? 'Deleting schedule and related backups…'
        : 'Deleting schedule…',
    )
    return completed && deleted
  }

  private unreferencedHistories(
    histories: T.ServiceTargetHistory[],
    job: T.BackupJob,
  ): T.ServiceTargetHistory[] {
    return histories.filter(
      history =>
        history.snapshots.length > 0 &&
        history.feedingJobs.length === 1 &&
        history.feedingJobs[0] === job.id,
    )
  }

  private historyBytes(history: T.ServiceTargetHistory): number {
    return history.snapshots.reduce(
      (sum, snapshot) => sum + (snapshot.physicalSize ?? snapshot.logicalSize),
      0,
    )
  }
}
