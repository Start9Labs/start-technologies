import { Component, inject, resource, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { RouterLink } from '@angular/router'
import {
  CopyService,
  i18nKey,
  i18nPipe,
  MokEnrollmentSteps,
  TaskService,
} from '@start9labs/shared'
import { RPC } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCheckbox,
  TuiIcon,
  TuiInput,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiPassword } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'

type Enrollment = RPC.RpcReturnType<RPC.StartOS, 'server.fde.begin-enable'>
type Certificate = Enrollment['certificates'][number]

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Full-disk encryption' | i18n }}
    </ng-container>
    <header tuiHeader>
      <h3 tuiTitle>{{ 'Full-disk encryption' | i18n }}</h3>
    </header>
    <p>
      {{
        'Supported on generic UEFI x86_64 and aarch64 servers with Secure Boot and TPM 2.0.'
          | i18n
      }}
    </p>
    @if (status.error()) {
      <div tuiNotification appearance="negative" role="alert">
        {{ 'Error' | i18n }}
      </div>
      <button tuiButton type="button" (click)="status.reload()">
        {{ 'Try again' | i18n }}
      </button>
    } @else if (status.hasValue()) {
      @if (status.value().supportedPlatform) {
        <div tuiNotification appearance="warning" role="alert">
          {{
            'After every restart, your server and its services will remain offline until you unlock it with your password.'
              | i18n
          }}
        </div>
        @if (progress(); as progress) {
          <section tuiForm="m">
            <h4>{{ 'Firmware enrollment' | i18n }}</h4>
            <div tuiNotification appearance="info">
              {{
                'Firmware enrollment does not activate disk protection.' | i18n
              }}
            </div>
            @if (progress.firmwareEnrolled) {
              <p>
                {{
                  'Firmware enrollment complete. Activation is pending.' | i18n
                }}
              </p>
            } @else {
              @if (progress.approvalCode; as code) {
                <tui-textfield>
                  <label tuiLabel>{{ 'Firmware approval code' | i18n }}</label>
                  <input tuiInput [readonly]="true" [value]="code" />
                  <button
                    tuiIconButton
                    type="button"
                    appearance="flat"
                    iconStart="@tui.copy"
                    (click)="copy.copy(code)"
                  >
                    {{ 'Copy' | i18n }}
                  </button>
                </tui-textfield>
              }
              <p>
                {{
                  'Review every pending key in firmware. This code also applies to other pending enrollment requests.'
                    | i18n
                }}
              </p>
              <p>
                {{
                  'This code approves firmware keys. It cannot unlock your data.'
                    | i18n
                }}
              </p>
              <p>
                {{
                  'Connect a monitor and keyboard to your server before rebooting.'
                    | i18n
                }}
              </p>
              <mok-enrollment-steps>
                {{ 'When prompted, enter the firmware approval code' | i18n }}
              </mok-enrollment-steps>
            }
            @for (
              certificate of progress.certificates;
              track certificate.purpose
            ) {
              <h5>
                {{ purposes[certificate.purpose] | i18n }} —
                {{ states[certificate.enrollment] | i18n }}
              </h5>
              <tui-textfield>
                <label tuiLabel>
                  {{ 'Fingerprint' | i18n }} (MokManager / SHA-1)
                </label>
                <input
                  tuiInput
                  [readonly]="true"
                  [value]="certificate.firmwareFingerprint"
                />
                <button
                  tuiIconButton
                  type="button"
                  appearance="flat"
                  iconStart="@tui.copy"
                  (click)="copy.copy(certificate.firmwareFingerprint)"
                >
                  {{ 'Copy' | i18n }}
                </button>
              </tui-textfield>
            }
          </section>
        }
        <form tuiForm="m" [formGroup]="form" (submit.prevent)="begin()">
          <tui-textfield>
            <label tuiLabel>{{ 'Password' | i18n }}</label>
            <input
              tuiInput
              type="password"
              autocomplete="current-password"
              formControlName="password"
            />
            <tui-icon tuiPassword />
          </tui-textfield>
          <label>
            <input
              tuiCheckbox
              type="checkbox"
              formControlName="acknowledgedOffline"
            />
            {{ 'I understand that every restart requires my password' | i18n }}
          </label>
          <footer>
            <button tuiButton [disabled]="formStatus() !== 'VALID'">
              {{
                (status.value().enableRequested
                  ? 'Continue enabling'
                  : 'Enable full-disk encryption'
                ) | i18n
              }}
            </button>
          </footer>
        </form>
      }
    } @else {
      <tui-loader />
    }
  `,
  imports: [
    ReactiveFormsModule,
    RouterLink,
    TuiButton,
    TuiCheckbox,
    TuiForm,
    TuiHeader,
    TuiIcon,
    TuiInput,
    TuiLoader,
    TuiNotification,
    TuiPassword,
    TuiTitle,
    TitleDirective,
    i18nPipe,
    MokEnrollmentSteps,
  ],
})
export default class Fde {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly copy = inject(CopyService)
  protected readonly status = resource({
    loader: () => this.api.rpc.server.fde.status({}),
  })
  protected readonly progress = signal<Enrollment | null>(null)
  protected readonly form = inject(NonNullableFormBuilder).group({
    password: ['', Validators.required],
    acknowledgedOffline: [false, Validators.requiredTrue],
  })
  protected readonly formStatus = toSignal(this.form.statusChanges, {
    initialValue: this.form.status,
  })
  protected readonly purposes = {
    image: 'Image signing',
    modules: 'Kernel module signing',
  } satisfies Record<Certificate['purpose'], i18nKey>
  protected readonly states = {
    enrolled: 'Enrolled',
    pending: 'Awaiting firmware approval',
    notEnrolled: 'Not enrolled',
  } satisfies Record<Certificate['enrollment'], i18nKey>

  protected async begin() {
    this.form.markAllAsTouched()
    if (this.form.invalid) return
    await this.tasks.run(async () => {
      this.progress.set(
        await this.api.rpc.server.fde.beginEnable(this.form.getRawValue()),
      )
      this.form.reset()
      this.status.reload()
    }, 'Enabling full-disk encryption')
  }
}
