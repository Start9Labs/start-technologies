import { DatePipe } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import {
  TUI_BREAKPOINT,
  TuiButton,
  TuiDialogContext,
  TuiNotification,
  TuiNotificationService,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { BackupService } from './backup.service'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Old backup' | i18n }}</h2>
    </header>
    <p>
      {{
        'This target still has a backup in the old "StartOSBackups" folder, which StartOS no longer uses. Deleting it frees up space.'
          | i18n
      }}
    </p>
    @if (latest(); as latest) {
      <p>
        {{ 'Your latest current backup is on' | i18n }}
        {{ latest.name }}, {{ latest.timestamp | date: 'mediumDate' }}.
      </p>
    } @else {
      <div tuiNotification appearance="warning">
        {{
          'This server has no current backup yet. Create one before deleting this.'
            | i18n
        }}
      </div>
    }
    <div class="actions">
      <button tuiButton iconStart="@tui.brush-cleaning" (click)="remove()">
        {{ 'Delete old backup' | i18n }}
      </button>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    p {
      margin: 0;
    }

    .actions {
      display: flex;
      justify-content: flex-end;
    }
  `,
  imports: [
    DatePipe,
    TuiButton,
    TuiHeader,
    TuiNotification,
    TuiTitle,
    i18nPipe,
  ],
})
class LegacyBackup {
  private readonly alerts = inject(TuiNotificationService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly service = inject(BackupService)

  protected readonly context = injectContext<TuiDialogContext<void, string>>()
  protected readonly latest = computed(() =>
    Object.values(this.service.targets())
      .flatMap(({ name, startOs }) => {
        const backup = startOs[this.service.serverId()]
        return backup?.timestamp && this.service.hasCurrentBackup({ startOs })
          ? [{ name, timestamp: backup.timestamp }]
          : []
      })
      .sort((a, b) => b.timestamp.localeCompare(a.timestamp))
      .at(0),
  )

  protected async remove() {
    const deleted = await this.tasks.run(
      () =>
        this.api.rpc.backup.target.deleteLegacy({
          targetId: this.context.data,
        }),
      'Deleting old backup',
    )
    if (!deleted) return
    this.context.$implicit.complete()
    this.alerts
      .open(
        this.i18n.transform(
          'Old backup deleted. Space will be reclaimed in the background.',
        ),
        { appearance: 'positive' },
      )
      .subscribe()
  }
}

const LEGACY_DELETE = new PolymorpheusComponent(LegacyBackup)

@Component({
  selector: 'backup-legacy-warning',
  template: `
    @if (breakpoint() === 'mobile') {
      <button
        tuiIconButton
        size="s"
        appearance="warning"
        iconStart="@tui.brush-cleaning"
        (click)="open()"
      >
        {{ 'Delete old backup' | i18n }}
      </button>
    } @else {
      <button
        tuiButton
        size="s"
        appearance="warning"
        iconStart="@tui.brush-cleaning"
        (click)="open()"
      >
        {{ 'Delete old backup' | i18n }}
      </button>
    }
  `,
  styles: `
    :host {
      display: inline-flex;
      vertical-align: middle;
    }
  `,
  imports: [TuiButton, i18nPipe],
})
export class BackupLegacyWarningComponent {
  private readonly dialogs = inject(DialogService)

  protected readonly breakpoint = inject(TUI_BREAKPOINT)

  readonly id = input.required<string>()

  protected open() {
    this.dialogs
      .openComponent(LEGACY_DELETE, {
        size: 's',
        data: this.id(),
      })
      .subscribe()
  }
}
