import { DecimalPipe } from '@angular/common'
import { Component, computed, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCell,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiLabel,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiButtonLoading, TuiPassword, TuiProgress } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm, TuiHeader } from '@taiga-ui/layout'
import { SetupFlash } from './service'
import { AuthService } from 'src/app/services/auth.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { getBrowserTimezone } from 'src/app/utils/timezones'

type Step = 'welcome' | 'password' | 'confirm' | 'flashing' | 'complete'
type FlashMode = 'update' | 'fresh-start'

function passwordsMatch(control: AbstractControl): ValidationErrors | null {
  const password = control.get('password')?.value
  const confirm = control.get('confirm')?.value
  if (password && confirm && password !== confirm) {
    control.get('confirm')?.setErrors({ mismatch: true })
    return { mismatch: true }
  }
  return null
}

@Component({
  template: `
    <img alt="StartWRT" src="assets/favicon.svg" />

    @switch (step()) {
      @case ('welcome') {
        <header tuiHeader>
          <hgroup tuiTitle>
            <h2>{{ 'StartWRT Setup' | i18n }}</h2>
            <p tuiSubtitle>{{ 'Choose how to set up your router' | i18n }}</p>
          </hgroup>
        </header>
        @if (canUpdate()) {
          <button tuiCell="l" (click)="selectMode('update')">
            <div tuiTitle>
              {{ 'Keep settings' | i18n }}
              <div tuiSubtitle>
                {{ 'Preserve your configuration, update firmware' | i18n }}
              </div>
            </div>
          </button>
        }
        <button tuiCell="l" (click)="selectMode('fresh-start')">
          <div tuiTitle>
            {{ 'Fresh Start' | i18n }}
            <div tuiSubtitle>
              {{
                'Erase all router configuration, install fresh firmware' | i18n
              }}
            </div>
          </div>
        </button>
        @if (!emmcFound()) {
          <tui-error [error]="'No eMMC device found' | i18n" />
        }
      }
      @case ('password') {
        <header tuiHeader>
          <h2 tuiTitle>{{ 'Create admin password' | i18n }}</h2>
        </header>
        <form tuiForm="m" [formGroup]="form" (ngSubmit)="onPasswordSubmit()">
          <tui-textfield>
            <label tuiLabel>{{ 'Password' | i18n }}</label>
            <input tuiInput formControlName="password" type="password" />
            <tui-icon tuiPassword />
          </tui-textfield>
          <tui-error formControlName="password" />
          <tui-textfield>
            <label tuiLabel>{{ 'Confirm password' | i18n }}</label>
            <input tuiInput formControlName="confirm" type="password" />
            <tui-icon tuiPassword />
          </tui-textfield>
          <tui-error formControlName="confirm" />
          <footer>
            <button
              tuiButton
              appearance="flat"
              type="button"
              (click)="step.set('welcome')"
            >
              {{ 'Back' | i18n }}
            </button>
            <button tuiButton appearance="primary" type="submit">
              {{ 'Next' | i18n }}
            </button>
          </footer>
        </form>
      }
      @case ('confirm') {
        <header tuiHeader>
          <hgroup tuiTitle>
            <h2>{{ 'Confirm' | i18n }}</h2>
            @if (mode() === 'update') {
              <p>
                {{
                  'Firmware will be updated. Your settings will be preserved.'
                    | i18n
                }}
              </p>
            } @else {
              <p>
                {{
                  'All router configuration (network, firewall, packages) will be erased and firmware freshly installed. Your admin password and WiFi credentials will be applied to the new installation.'
                    | i18n
                }}
              </p>
            }
            <p tuiSubtitle>{{ 'This will take a few minutes.' | i18n }}</p>
          </hgroup>
        </header>
        <footer>
          <button tuiButton appearance="flat" (click)="step.set('password')">
            {{ 'Back' | i18n }}
          </button>
          <button
            tuiButton
            appearance="primary"
            autofocus
            [loading]="flash.running()"
            (click)="startFlash()"
          >
            {{ 'Flash' | i18n }}
          </button>
        </footer>
      }
      @case ('flashing') {
        <header tuiHeader>
          <h2 tuiTitle>{{ 'Flashing' | i18n }}</h2>
        </header>
        <div>{{ flash.status() }}</div>
        @if (flash.totalSteps() > 0) {
          <progress
            tuiProgressBar
            [max]="flash.totalSteps()"
            [value]="flash.step() - 1"
          ></progress>
          <small>
            {{ 'Step' | i18n }} {{ flash.step() }} {{ 'of' | i18n }}
            {{ flash.totalSteps() }}
          </small>
        }
        @if (flash.total() > 0) {
          <progress
            tuiProgressBar
            [max]="flash.total()"
            [value]="flash.copied()"
          ></progress>
          <small>
            {{ flash.copied() / 1000000 | number: '1.0-0' }} /
            {{ flash.total() / 1000000 | number: '1.0-0' }} MB
          </small>
        }
        @if (flash.error()) {
          <tui-error [error]="flash.error()" />
          <footer>
            <button tuiButton appearance="flat" (click)="step.set('confirm')">
              {{ 'Try Again' | i18n }}
            </button>
          </footer>
        }
      }
      @case ('complete') {
        <header tuiHeader>
          <h2 tuiTitle>{{ 'Setup complete' | i18n }}</h2>
        </header>
        <div>
          {{ 'Remove the microSD card and reboot your router.' | i18n }}
        </div>
      }
    }
  `,
  styles: `
    :host {
      position: absolute;
      inline-size: 20rem;
      inset-inline-start: 50%;
      inset-block-start: 50%;
      transform: translate(-50%, -50%);
    }

    img {
      width: 5rem;
      height: 5rem;
      margin: auto;
    }

    small {
      margin-block-start: -0.75rem !important;
      color: var(--tui-text-secondary);
    }

    footer {
      display: flex;
      justify-content: space-between;
    }

    :host-context(body:not([tuiTheme])) img {
      filter: invert(1);
    }
  `,
  hostDirectives: [TuiCardLarge],
  providers: [
    SetupFlash,
    provideTranslatedValidationErrors({
      required: 'This field is required',
      minlength: 'Password must be at least 12 characters',
      mismatch: 'Passwords do not match',
    }),
  ],
  imports: [
    DecimalPipe,
    ReactiveFormsModule,
    TuiButton,
    TuiButtonLoading,
    TuiCell,
    TuiError,
    TuiIcon,
    TuiInput,
    TuiLabel,
    TuiPassword,
    TuiProgress,
    TuiTextfield,
    TuiTitle,
    TuiHeader,
    TuiForm,
    i18nPipe,
  ],
})
export default class SetupWizard {
  private readonly auth = inject(AuthService)
  protected readonly flash = inject(SetupFlash)

  protected readonly step = signal<Step>('welcome')
  protected readonly mode = signal<FlashMode>('fresh-start')

  protected readonly emmcFound = computed(
    () => this.auth.setupDisk()?.emmcFound ?? false,
  )
  protected readonly canUpdate = computed(
    () => this.auth.setupDisk()?.hasFirmware ?? false,
  )

  public readonly form = inject(NonNullableFormBuilder).group(
    {
      password: ['', [Validators.required, Validators.minLength(12)]],
      confirm: ['', Validators.required],
    },
    { validators: passwordsMatch },
  )

  selectMode(mode: FlashMode): void {
    this.mode.set(mode)
    this.step.set('password')
  }

  onPasswordSubmit(): void {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      return
    }
    this.step.set('confirm')
  }

  async startFlash(): Promise<void> {
    this.step.set('flashing')
    if (
      await this.flash.run({
        mode: this.mode(),
        password: this.form.getRawValue().password,
        timezone: getBrowserTimezone(),
      })
    )
      this.step.set('complete')
  }
}
