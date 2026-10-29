import { Component, inject, signal } from '@angular/core'
import {
  FormsModule,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TUI_VALIDATION_ERRORS,
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiPassword, TuiRadioList } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { ApiService } from 'src/app/services/api/embassy-api.service'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">{{ 'New volume' | i18n }}</h2>
      </header>
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input tuiInput formControlName="name" />
      </tui-textfield>
      <tui-error formControlName="name" />
      <fieldset>
        <legend>{{ 'Source' | i18n }}</legend>
        <tui-radio-list
          [items]="sources"
          [itemContent]="sourceContent"
          [ngModelOptions]="{ standalone: true }"
          [(ngModel)]="source"
        />
        <ng-template #sourceContent let-item>{{ item | i18n }}</ng-template>
      </fieldset>
      @if (source() === 'CIFS share') {
        <tui-textfield>
          <label tuiLabel>{{ 'Hostname' | i18n }}</label>
          <input
            tuiInput
            formControlName="hostname"
            required
            autocapitalize="off"
          />
        </tui-textfield>
        <tui-error formControlName="hostname" />
        <tui-textfield>
          <label tuiLabel>{{ 'Path' | i18n }}</label>
          <input
            tuiInput
            formControlName="path"
            required
            autocapitalize="off"
          />
        </tui-textfield>
        <tui-error formControlName="path" />
        <tui-textfield>
          <label tuiLabel>{{ 'Username' | i18n }}</label>
          <input
            tuiInput
            formControlName="username"
            autocomplete="off"
            autocapitalize="off"
          />
        </tui-textfield>
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input
            tuiInput
            type="password"
            formControlName="password"
            autocomplete="new-password"
          />
          <tui-icon tuiPassword />
        </tui-textfield>
      }
      <p>
        {{
          'The source cannot be changed after creation. Credentials remain private.'
            | i18n
        }}
      </p>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  providers: [
    {
      provide: TUI_VALIDATION_ERRORS,
      useFactory: () => {
        const i18n = inject(i18nPipe)
        return { required: () => i18n.transform('Required') }
      },
    },
  ],
  imports: [
    FormsModule,
    ReactiveFormsModule,
    i18nPipe,
    TuiButton,
    TuiError,
    TuiIcon,
    TuiInput,
    TuiTitle,
    TuiPassword,
    TuiRadioList,
    TuiForm,
    TuiHeader,
  ],
})
class CreateVolume {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly context = injectContext<TuiDialogContext<void>>()
  protected readonly sources = ['Local folder', 'CIFS share'] as const
  protected readonly source =
    signal<(typeof this.sources)[number]>('Local folder')
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', Validators.required],
    hostname: [''],
    path: [''],
    username: [''],
    password: [''],
  })

  protected async save() {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid) return
    const { name, hostname, path, username, password } = this.form.getRawValue()
    if (
      await this.tasks.run(
        () =>
          this.api.rpc.volumes.create({
            name,
            source:
              this.source() === 'Local folder'
                ? { kind: 'local' }
                : { kind: 'cifs', hostname, path, username, password },
          }),
        'Saving',
      )
    )
      this.context.$implicit.complete()
  }
}

export const CREATE_VOLUME = new PolymorpheusComponent(CreateVolume)
