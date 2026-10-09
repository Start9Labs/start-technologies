import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiDialogService,
  TuiInput,
} from '@taiga-ui/core'
import { TuiBadge, TuiSwitch } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { from, map } from 'rxjs'

import { BackupJob } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ToHumanCronPipe } from '../pipes/to-human-cron.pipe'
import { BackupJobBuilder } from '../utils/job-builder'
import { BACKUP, BACKUP_OPTIONS } from './backup.component'
import { TARGET, TARGET_CREATE } from './target.component'

@Component({
  template: `
    <form class="form">
      <tui-textfield>
        <label tuiLabel>Job Name</label>
        <input
          tuiInput
          name="name"
          [(ngModel)]="job.name"
          placeholder="My Backup Job"
        />
      </tui-textfield>
      <button
        tuiButton
        appearance="primary"
        type="button"
        class="button"
        size="l"
        (click)="selectTarget()"
      >
        Target
        <span
          tuiBadge
          [appearance]="target()?.[targetId()]?.type ? 'success' : 'warning'"
        >
          {{ target()?.[targetId()]?.type || 'Select target' }}
        </span>
      </button>
      <button
        tuiButton
        appearance="primary"
        type="button"
        class="button"
        size="l"
        (click)="selectPackages()"
      >
        Packages
        <span
          tuiBadge
          [appearance]="packageIds().length ? 'success' : 'warning'"
        >
          {{ packageIds().length + ' selected' }}
        </span>
      </button>
      <tui-textfield>
        <label tuiLabel>Schedule</label>
        <input
          tuiInput
          name="cron"
          [(ngModel)]="job.cron"
          placeholder="* * * * *"
        />
      </tui-textfield>
      @if (job.cron | toHumanCron; as human) {
        <div [style.color]="human.color">{{ human.message }}</div>
      }
      @if (!job.job.id) {
        <div class="g-toggle">
          Also Execute Now
          <input tuiSwitch type="checkbox" name="now" [(ngModel)]="job.now" />
        </div>
      }
      <button
        tuiButton
        class="submit"
        [style.margin-left]="'auto'"
        (click)="save()"
      >
        Save Job
      </button>
    </form>
  `,
  styles: `
    .form {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    .button[data-size] {
      inline-size: unset;
      padding: 1rem;
      text-indent: 0;
      justify-content: space-between;
    }
  `,
  imports: [
    FormsModule,
    TuiInput,
    TuiSwitch,
    TuiButton,
    TuiBadge,
    ToHumanCronPipe,
  ],
})
export class BackupsEditModal {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(TuiDialogService)
  private readonly context =
    injectContext<TuiDialogContext<BackupJob, BackupJobBuilder>>()

  protected readonly target = toSignal(
    from(this.api.getBackupTargets({})).pipe(map(({ saved }) => saved)),
  )

  protected readonly targetId = signal(this.job.targetId)
  protected readonly packageIds = signal(this.job.packageIds)

  protected get job() {
    return this.context.data
  }

  protected async save() {
    this.tasks.run(async () => {
      const job = this.job.job.id
        ? await this.api.updateBackupJob(this.job.buildUpdate(this.job.job.id))
        : await this.api.createBackupJob(this.job.buildCreate())

      this.context.completeWith(job)
    }, 'Saving Job')
  }

  protected selectTarget() {
    this.dialogs
      .open<T.BackupTarget & { id: string }>(TARGET, TARGET_CREATE)
      .subscribe(({ id }) => {
        this.job.targetId = id
        this.targetId.set(id)
      })
  }

  protected selectPackages() {
    this.dialogs.open<string[]>(BACKUP, BACKUP_OPTIONS).subscribe(id => {
      this.job.packageIds = id
      this.packageIds.set(id)
    })
  }
}

export const EDIT = new PolymorpheusComponent(BackupsEditModal)
