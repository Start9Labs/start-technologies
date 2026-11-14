import { Component, computed, effect, inject } from '@angular/core'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { TuiConfirmService } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext } from '@taiga-ui/polymorpheus'

import { FormDialogService } from 'src/app/services/form-dialog.service'
import { SOURCE_MANAGE_STYLES } from 'src/app/routes/portal/routes/system/routes/volumes/manage.styles'
import {
  SourceFields,
  sourceEdit,
} from 'src/app/routes/portal/routes/system/routes/volumes/source-fields'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { BackupService } from './backup.service'

@Component({
  template: `
    @if (target(); as target) {
      <div tuiForm="l">
        <source-fields [edit]="edit" />
        <footer>
          <button
            tuiButton
            type="button"
            appearance="flat-destructive"
            iconStart="@tui.trash-2"
            class="remove"
            (click)="context.completeWith(true)"
          >
            {{ (target.source.kind === 'local' ? 'Delete' : 'Remove') | i18n }}
          </button>
          <button
            tuiButton
            type="button"
            appearance="flat-grayscale"
            (click)="formDialog.close(context.$implicit)"
          >
            {{ 'Cancel' | i18n }}
          </button>
          <button
            tuiButton
            type="button"
            [disabled]="!edit.changed() || edit.invalid()"
            (click)="save()"
          >
            {{ 'Save' | i18n }}
          </button>
        </footer>
      </div>
    }
  `,
  styles: SOURCE_MANAGE_STYLES,
  imports: [SourceFields, TuiButton, TuiForm, i18nPipe],
})
export class BackupTargetManage {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  protected readonly formDialog = inject(FormDialogService)
  private readonly confirm = inject(TuiConfirmService, { optional: true })

  protected readonly service = inject(BackupService)
  protected readonly context =
    injectContext<TuiDialogContext<boolean, { id: string }>>()

  protected readonly target = computed(
    () => this.service.targets()[this.context.data.id],
  )
  protected readonly edit = sourceEdit(this.target)

  constructor() {
    effect(() =>
      this.edit.changed()
        ? this.confirm?.markAsDirty()
        : this.confirm?.markAsPristine(),
    )
  }

  protected async save() {
    if (
      !(await this.tasks.run(
        () => this.edit.save(this.context.data.id, this.api.rpc.backup.target),
        'Saving',
      ))
    )
      return
    this.confirm?.markAsPristine()
    this.context.$implicit.complete()
  }
}
