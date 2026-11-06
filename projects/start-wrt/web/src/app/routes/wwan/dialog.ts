import { Component, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { tuiMarkControlAsTouchedAndValidate, TuiAutoFocus } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDataList,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiNotification,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
  TuiTitle,
} from '@taiga-ui/core'
import type { TuiContext } from '@taiga-ui/cdk'
import { TuiChevron, TuiPassword, TuiRadioList, TuiSelect } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { startWith } from 'rxjs'
import {
  WwanConnectRequest,
  WwanNetwork,
  WwanPriority,
  WwanSecurity,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { BAND_LABELS, PRIORITY_LABELS, SECURITY_LABELS } from './utils'

const SSID_MAX_LENGTH = 32

export interface WwanJoinData {
  radio: string
  band: string
  /** Absent for a network that hides its name. */
  network?: WwanNetwork
}

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'Join network' | i18n }}</h2>
        <p tuiSubtitle>{{ band | i18n }}</p>
      </hgroup>
    </header>
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="join()">
      <tui-textfield>
        <label tuiLabel>{{ 'Network name' | i18n }}</label>
        <input
          tuiInput
          formControlName="ssid"
          [readOnly]="!!network?.ssid"
          [tuiAutoFocus]="!network?.ssid && !isMobile"
        />
      </tui-textfield>
      <tui-error formControlName="ssid" />
      @if (!network) {
        <tui-textfield tuiChevron [stringify]="stringifySecurity">
          <label tuiLabel>{{ 'Security' | i18n }}</label>
          <input tuiSelect formControlName="security" />
          <tui-data-list *tuiDropdown>
            @for (s of securities; track s) {
              <button tuiOption [value]="s">
                {{ stringifySecurity(s) }}
              </button>
            }
          </tui-data-list>
        </tui-textfield>
      }
      @if (security() !== 'open') {
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input
            tuiInput
            type="password"
            formControlName="password"
            [tuiAutoFocus]="!!network?.ssid && !isMobile"
          />
          <tui-icon tuiPassword />
        </tui-textfield>
        <tui-error formControlName="password" />
      }
      <fieldset>
        <legend>{{ 'Priority' | i18n }}</legend>
        <tui-radio-list
          formControlName="priority"
          [items]="priorities"
          [itemContent]="priority"
        />
      </fieldset>
      <div tuiNotification appearance="warning">
        {{
          'While the router is not connected to this network, its own Wi-Fi on this band is off. Join from a wired device or from the other band.'
            | i18n
        }}
      </div>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Join' | i18n }}</button>
      </footer>
    </form>
  `,
  providers: [
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
    provideTranslatedValidationErrors({
      required: 'This field is required',
      minlength: 'Must be 8 to 63 characters',
      maxlength: ({ requiredLength }) =>
        requiredLength === SSID_MAX_LENGTH
          ? 'Must be at most 32 characters'
          : 'Must be 8 to 63 characters',
    }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiForm,
    TuiTextfield,
    TuiInput,
    TuiError,
    TuiIcon,
    TuiPassword,
    TuiSelect,
    TuiChevron,
    TuiDataList,
    TuiRadioList,
    TuiNotification,
    TuiButton,
    TuiAutoFocus,
    i18nPipe,
  ],
})
class WwanJoinDialog {
  protected readonly context =
    injectContext<TuiDialogContext<WwanConnectRequest, WwanJoinData>>()
  private readonly i18n = inject(i18nPipe)
  protected readonly isMobile = inject(WA_IS_MOBILE)
  protected readonly network = this.context.data.network
  protected readonly band = BAND_LABELS[this.context.data.band] ?? 'Wi-Fi'

  protected readonly securities: WwanSecurity[] = [
    'wpa2',
    'wpa2Wpa3',
    'wpa3',
    'open',
  ]
  protected readonly priorities: WwanPriority[] = ['backup', 'preferred']

  protected readonly form = inject(NonNullableFormBuilder).group({
    ssid: [
      this.network?.ssid ?? '',
      [Validators.required, Validators.maxLength(SSID_MAX_LENGTH)],
    ],
    security: [this.network?.security ?? ('wpa2' as WwanSecurity)],
    password: [
      '',
      [Validators.required, Validators.minLength(8), Validators.maxLength(63)],
    ],
    priority: ['backup' as WwanPriority],
  })

  protected readonly security = toSignal(
    this.form.controls.security.valueChanges.pipe(
      startWith(this.form.controls.security.value),
    ),
    { requireSync: true },
  )

  protected readonly stringifySecurity = (s: WwanSecurity): string =>
    s === 'open' ? this.i18n.transform('Open') : SECURITY_LABELS[s]

  protected readonly priority = ({ $implicit }: TuiContext<WwanPriority>) =>
    this.i18n.transform(PRIORITY_LABELS[$implicit])

  constructor() {
    // An open network takes no password.
    effect(() => {
      const password = this.form.controls.password
      this.security() === 'open' ? password.disable() : password.enable()
    })
  }

  protected join(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid) return
    const { ssid, security, password, priority } = this.form.getRawValue()
    this.context.completeWith({
      radio: this.context.data.radio,
      ssid,
      security,
      password: security === 'open' ? null : password,
      priority,
    })
  }
}

export const WWAN_JOIN = new PolymorpheusComponent(WwanJoinDialog)
