import { Component, computed, effect, inject } from '@angular/core'
import { FormsModule } from '@angular/forms'
import {
  CopyService,
  DialogService,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiDialogContext, TuiIcon, TuiInput } from '@taiga-ui/core'
import { TuiConfirmService } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext } from '@taiga-ui/polymorpheus'
import { firstValueFrom } from 'rxjs'

import {
  SourceFields,
  sourceEdit,
} from 'src/app/routes/portal/routes/system/routes/volumes/source-fields'
import { VOLUME_TYPES } from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { BackupService } from './backup.service'
import { targetRpc } from './backup.types'

@Component({
  template: `
    @if (target(); as target) {
      <div class="context">
        <tui-icon [icon]="types[target.source.kind].icon" />
        {{ target.name }}
      </div>
      <div tuiForm="l">
        <source-fields [edit]="edit" [localPath]="localPath()" />
        <tui-textfield>
          <label tuiLabel>{{ 'Target ID' | i18n }}</label>
          <input tuiInput [readonly]="true" [ngModel]="context.data.id" />
          <button
            tuiIconButton
            appearance="icon"
            size="s"
            type="button"
            tabindex="-1"
            iconStart="@tui.copy"
            (pointerdown.stop)="(0)"
            (click)="copy.copy(context.data.id)"
          >
            {{ 'Copy' | i18n }}
          </button>
        </tui-textfield>
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
            (click)="cancel()"
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
  styles: `
    .context {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      margin-block-end: 1.5rem;
      font-weight: bold;

      tui-icon {
        font-size: 1.25rem;
      }
    }

    footer {
      position: sticky;
      bottom: 0;
      z-index: 10;
      display: flex;
      justify-content: flex-end;
      gap: 1rem;
      padding: 1rem 0;
      margin: 1rem -1px -1rem;
      background: var(--tui-background-elevation-1);
      box-shadow: inset 0 1px var(--tui-background-neutral-1);
    }

    .remove {
      margin-inline-end: auto;
    }
  `,
  imports: [
    FormsModule,
    SourceFields,
    TuiButton,
    TuiForm,
    TuiIcon,
    TuiInput,
    i18nPipe,
  ],
})
export class BackupTargetManage {
  protected readonly types = VOLUME_TYPES
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly confirm = inject(TuiConfirmService, { optional: true })

  protected readonly service = inject(BackupService)
  protected readonly copy = inject(CopyService)
  protected readonly context =
    injectContext<TuiDialogContext<boolean, { id: string }>>()

  protected readonly target = computed(
    () => this.service.targets()[this.context.data.id],
  )
  protected readonly edit = sourceEdit(this.target)
  protected readonly localPath = computed(
    () => `/media/startos/data/backup-targets/${this.context.data.id}`,
  )

  constructor() {
    effect(() =>
      this.edit.changed()
        ? this.confirm?.markAsDirty()
        : this.confirm?.markAsPristine(),
    )
  }

  protected async cancel() {
    if (
      this.edit.changed() &&
      !(await firstValueFrom(
        this.dialogs.openConfirm({
          label: 'Unsaved changes',
          size: 's',
          data: {
            content:
              'You have unsaved changes. Are you sure you want to leave?',
            yes: 'Leave',
            no: 'Cancel',
          },
        }),
        { defaultValue: false },
      ))
    )
      return
    this.context.$implicit.complete()
  }

  protected async save() {
    if (
      !(await this.tasks.run(
        () => this.edit.save(this.context.data.id, targetRpc(this.api)),
        'Saving',
      ))
    )
      return
    this.confirm?.markAsPristine()
    this.context.$implicit.complete()
  }
}
