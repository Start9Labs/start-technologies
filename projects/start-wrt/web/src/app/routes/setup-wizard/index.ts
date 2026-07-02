import { DecimalPipe } from '@angular/common'
import {
  Component,
  computed,
  inject,
  linkedSignal,
  signal,
} from '@angular/core'
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
import { SetupFlashEvent } from 'src/app/services/api/api.service'
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
            [loading]="flashing()"
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
        <div>{{ flashStatus() }}</div>
        @if (flashTotalSteps() > 0) {
          <progress
            tuiProgressBar
            [max]="flashTotalSteps()"
            [value]="flashStep() - 1"
          ></progress>
          <small>
            {{ 'Step' | i18n }} {{ flashStep() }} {{ 'of' | i18n }}
            {{ flashTotalSteps() }}
          </small>
        }
        @if (flashTotal() > 0) {
          <progress
            tuiProgressBar
            [max]="flashTotal()"
            [value]="flashCopied()"
          ></progress>
          <small>
            {{ flashCopied() / 1000000 | number: '1.0-0' }} /
            {{ flashTotal() / 1000000 | number: '1.0-0' }} MB
          </small>
        }
        @if (flashError()) {
          <tui-error [error]="flashError()" />
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
  private readonly i18n = inject(i18nPipe)

  protected readonly step = signal<Step>('welcome')
  protected readonly mode = signal<FlashMode>('fresh-start')
  protected readonly flashing = signal(false)
  protected readonly flashStatus = signal(this.i18n.transform('Preparing...'))
  protected readonly flashStep = signal(0)
  protected readonly flashTotalSteps = signal(0)
  protected readonly flashCopied = signal(0)
  protected readonly flashTotal = signal(0)
  protected readonly flashError = linkedSignal<string | null>(() => null)

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
    this.flashing.set(true)
    this.flashError.set(null)
    this.flashStep.set(0)
    this.flashTotalSteps.set(0)
    this.flashCopied.set(0)
    this.flashTotal.set(0)
    this.flashStatus.set(this.i18n.transform('Starting flash...'))
    this.step.set('flashing')

    try {
      const response = await fetch('/api/setup/flash', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          mode: this.mode(),
          password: this.form.value.password,
          // Carried into the fresh eMMC config; the backend resolves the POSIX
          // string and leaves UTC if the zone is unknown (FreshStart only).
          timezone: getBrowserTimezone(),
        }),
      })

      if (!response.ok || !response.body) {
        throw new Error(`HTTP ${response.status}`)
      }

      const reader = response.body.getReader()
      const decoder = new TextDecoder()
      let buffer = ''

      while (true) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })
        const lines = buffer.split('\n')
        buffer = lines.pop()!

        for (const line of lines) {
          if (!line.trim()) continue
          const event: SetupFlashEvent = JSON.parse(line)
          this.handleFlashEvent(event)
        }
      }

      // Process any remaining buffer
      if (buffer.trim()) {
        const event: SetupFlashEvent = JSON.parse(buffer)
        this.handleFlashEvent(event)
      }
    } catch (e: any) {
      this.flashError.set(e?.message || this.i18n.transform('Flash failed'))
    } finally {
      this.flashing.set(false)
    }
  }

  private handleFlashEvent(event: SetupFlashEvent): void {
    if (event.step) this.flashStep.set(event.step)
    if (event.totalSteps) this.flashTotalSteps.set(event.totalSteps)

    switch (event.phase) {
      case 'copying':
        this.flashStatus.set(this.i18n.transform('Copying firmware...'))
        this.flashCopied.set(event.copied ?? 0)
        this.flashTotal.set(event.total ?? 0)
        break
      case 'status':
        this.flashStatus.set(event.message ?? '')
        this.flashTotal.set(0)
        break
      case 'complete':
        this.step.set('complete')
        break
      case 'error':
        this.flashError.set(
          event.message ?? this.i18n.transform('Unknown error'),
        )
        break
    }
  }
}
