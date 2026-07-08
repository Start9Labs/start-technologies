import { Component, inject, input } from '@angular/core'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiNotificationService } from '@taiga-ui/core'
import { filter, of, switchMap } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { BackupService } from './backup.service'

@Component({
  selector: 'backup-legacy-warning',
  template: `
    <button
      tuiButton
      size="s"
      iconStart="@tui.brush-cleaning"
      (click)="$event.stopPropagation(); remove()"
    >
      {{ 'Delete old backup' | i18n }}
    </button>
  `,
  styles: `
    :host {
      display: inline-flex;
    }
  `,
  imports: [TuiButton, i18nPipe],
})
export class BackupLegacyWarningComponent {
  private readonly dialog = inject(DialogService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly service = inject(BackupService)
  private readonly i18n = inject(i18nPipe)

  readonly id = input.required<string>()
  readonly hasCurrentBackup = input.required<boolean>()

  remove() {
    const extraConfirm = !this.hasCurrentBackup()

    this.dialog
      .openConfirm({
        label: 'Delete old backup?',
        size: 's',
        data: {
          content: extraConfirm
            ? 'Permanently delete the old (V1) backup from this target? This cannot be undone.'
            : 'Permanently delete the old (V1) backup from this target? This cannot be undone. Your current (V2) backup will not be affected.',
          no: 'Cancel',
          yes: 'Delete',
        },
      })
      .pipe(
        filter(Boolean),
        switchMap(() =>
          extraConfirm
            ? this.dialog.openConfirm({
                label: 'This target has no other backup',
                size: 's',
                data: {
                  content:
                    'There is no current (V2) backup for this server on this target, so deleting the old (V1) backup will leave this server with no backup here. Continue?',
                  no: 'Cancel',
                  yes: 'Delete anyway',
                },
              })
            : of(true),
        ),
        filter(Boolean),
      )
      .subscribe(() =>
        this.tasks.run(async () => {
          await this.api.deleteLegacyBackup({ targetId: this.id() })
          this.service.clearLegacy(this.id())
          this.alerts
            .open(this.i18n.transform('Old backup deleted'), {
              appearance: 'positive',
            })
            .subscribe()
        }, 'Deleting old backup'),
      )
  }
}
