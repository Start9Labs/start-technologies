import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { Router } from '@angular/router'
import {
  hostnameValidationErrors,
  hostnameValidator,
  i18nPipe,
  randomHostname,
  TaskService,
} from '@start9labs/shared'
import { TuiMapperPipe, TuiValidator } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiTitle,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { TuiPassword } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm, TuiHeader } from '@taiga-ui/layout'
import { map } from 'rxjs'
import { StateService } from '../services/state.service'

@Component({
  template: `
    <form
      tuiCardLarge="compact"
      tuiForm
      [formGroup]="form"
      (ngSubmit)="submit()"
    >
      <header tuiHeader>
        <h2 tuiTitle>
          {{
            isFresh
              ? ('Set Up Your Server' | i18n)
              : ('Set New Password (Optional)' | i18n)
          }}
        </h2>
      </header>

      @if (isFresh) {
        <tui-textfield>
          <label tuiLabel>{{ 'Server Name' | i18n }}</label>
          <input tuiInput autocapitalize="off" formControlName="hostname" />
          <button
            tuiIconButton
            type="button"
            appearance="icon"
            iconStart="@tui.refresh-cw"
            (click)="randomizeHostname()"
          ></button>
        </tui-textfield>
        <tui-error formControlName="hostname" />
        @if (form.controls.hostname.valid) {
          <tui-error class="g-secondary" error="{{ hostname() }}.local" />
        }
      }

      <tui-textfield>
        <label tuiLabel>
          {{ isFresh ? ('Password' | i18n) : ('New Password' | i18n) }}
        </label>
        <input
          tuiInput
          type="password"
          maxlength="64"
          formControlName="password"
        />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="password" />

      <tui-textfield>
        <label tuiLabel>{{ 'Confirm Password' | i18n }}</label>
        <input
          tuiInput
          type="password"
          formControlName="confirm"
          [tuiValidator]="
            form.controls.password.value || '' | tuiMapper: validator
          "
        />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="confirm" />

      <footer>
        @if (!isFresh) {
          <button
            tuiButton
            size="m"
            appearance="secondary"
            type="button"
            (click)="skip()"
          >
            {{ 'Skip' | i18n }}
          </button>
        }
        <button tuiButton size="m" [disabled]="form.invalid">
          {{ 'Finish' | i18n }}
        </button>
      </footer>
    </form>
  `,
  imports: [
    ReactiveFormsModule,
    TuiCardLarge,
    TuiButton,
    TuiError,
    TuiInput,
    TuiForm,
    TuiPassword,
    TuiValidator,
    TuiIcon,
    TuiMapperPipe,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)

      return {
        ...hostnameValidationErrors(),
        required: i18n.transform('Required'),
        minlength: i18n.transform('Must be 12 characters or greater'),
        maxlength: i18n.transform('Must be 64 character or less'),
        match: i18n.transform('Passwords do not match'),
      }
    }),
  ],
})
export default class PasswordPage {
  private readonly router = inject(Router)
  private readonly tasks = inject(TaskService)
  private readonly stateService = inject(StateService)
  private readonly i18n = inject(i18nPipe)

  readonly isFresh = this.stateService.setupType === 'fresh'

  readonly form = inject(NonNullableFormBuilder).group({
    password: [
      '',
      [
        ...(this.isFresh ? [Validators.required] : []),
        Validators.minLength(12),
        Validators.maxLength(64),
      ],
    ],
    confirm: [''],
    hostname: [
      this.isFresh ? randomHostname() : '',
      this.isFresh ? [hostnameValidator] : [],
    ],
  })

  readonly validator = (value: string) => (control: AbstractControl) =>
    value === control.value
      ? null
      : { match: this.i18n.transform('Passwords do not match') }

  readonly hostname = toSignal(
    this.form.controls.hostname.valueChanges.pipe(map(value => value.trim())),
    { initialValue: this.form.getRawValue().hostname.trim() },
  )

  randomizeHostname() {
    this.form.controls.hostname.setValue(randomHostname())
  }

  async skip() {
    await this.executeSetup(null)
  }

  async submit() {
    await this.executeSetup(this.form.controls.password.value)
  }

  private async executeSetup(password: string | null) {
    const hostname = this.hostname()

    this.tasks.run(async () => {
      if (this.stateService.setupType === 'attach') {
        await this.stateService.attachDrive(password)
      } else {
        await this.stateService.executeSetup(password, hostname)
      }

      await this.router.navigate(['/loading'])
    }, 'Starting setup')
  }
}
