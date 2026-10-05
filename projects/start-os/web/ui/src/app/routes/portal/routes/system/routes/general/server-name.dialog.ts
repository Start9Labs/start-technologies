import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import {
  hostnameValidationErrors,
  hostnameValidator,
  i18nPipe,
  randomHostname,
} from '@start9labs/shared'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { map } from 'rxjs'

@Component({
  template: `
    <form [formGroup]="form" (submit.prevent)="save()">
      <tui-textfield>
        <label tuiLabel>{{ 'Server Name' | i18n }}</label>
        <input tuiInput autocapitalize="off" formControlName="hostname" />
        <button
          tuiIconButton
          type="button"
          appearance="icon"
          iconStart="@tui.refresh-cw"
          (click)="randomize()"
        ></button>
      </tui-textfield>
      <tui-error formControlName="hostname" />
      @if (form.valid) {
        <p class="hostname-preview">{{ hostname() }}.local</p>
      }
      <div class="used-for">
        <p>{{ 'Used for:' | i18n }}</p>
        <ul>
          <li>{{ 'The address of your server' | i18n }}</li>
          <li>{{ 'The local address of every installed service' | i18n }}</li>
          <li>
            {{ 'The browser tab and the name of the installed app' | i18n }}
          </li>
          <li>{{ 'SSH and system logs' | i18n }}</li>
        </ul>
      </div>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          (click)="context.completeWith(null)"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton [disabled]="form.invalid">{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  styles: `
    .hostname-preview {
      color: var(--tui-text-secondary);
      font: var(--tui-typography-body-s);
      margin-top: 0.25rem;
    }

    .used-for {
      color: var(--tui-text-secondary);
      font: var(--tui-typography-body-s);
      margin-top: 1rem;

      p {
        margin: 0;
      }

      ul {
        margin: 0.25rem 0 0;
        padding-inline-start: 1.25rem;
      }
    }

    footer {
      display: flex;
      gap: 1rem;
      margin-top: 1.5rem;
    }
  `,
  imports: [ReactiveFormsModule, TuiButton, TuiError, TuiInput, i18nPipe],
  providers: [tuiValidationErrorsProvider(hostnameValidationErrors)],
})
export class ServerNameDialog {
  protected readonly context =
    injectContext<TuiDialogContext<string | null, { hostname: string }>>()

  protected readonly form = inject(NonNullableFormBuilder).group({
    hostname: [this.context.data.hostname, hostnameValidator],
  })

  protected readonly hostname = toSignal(
    this.form.controls.hostname.valueChanges.pipe(map(value => value.trim())),
    { initialValue: this.context.data.hostname.trim() },
  )

  constructor() {
    // `tui-error` renders nothing until the control is touched.
    if (this.form.invalid) this.form.markAllAsTouched()
  }

  protected randomize() {
    this.form.controls.hostname.setValue(randomHostname())
  }

  protected save() {
    this.context.completeWith(this.hostname())
  }
}
