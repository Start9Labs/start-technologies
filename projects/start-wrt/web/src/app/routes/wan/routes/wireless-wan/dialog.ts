import { Component, inject, signal } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  tuiMarkControlAsTouchedAndValidate,
  TuiAutoFocus,
  TuiValueChanges,
} from '@taiga-ui/cdk'
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
import {
  WwanConnectRequest,
  WwanNetwork,
  WwanPriority,
  WwanSecurity,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import {
  BAND_LABELS,
  byteLength,
  noControlCharacters,
  PRIORITY_LABELS,
  securityLabel,
} from './utils'

export interface WwanJoinData {
  radio: string
  band: string
  /** Whether another band's Wi-Fi stays up during the join. */
  otherBand: boolean
  network?: WwanNetwork
  priority?: WwanPriority
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
          <input
            tuiSelect
            formControlName="security"
            (tuiValueChanges)="onSecurity($event)"
          />
          <tui-data-list *tuiDropdown>
            @for (s of securities; track s) {
              <button tuiOption [value]="s">
                {{ stringifySecurity(s) }}
              </button>
            }
          </tui-data-list>
        </tui-textfield>
      }
      @if (form.controls.password.enabled) {
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input
            tuiInput
            type="password"
            autocomplete="new-password"
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
          'While the router is not connected to this network, its own Wi-Fi on this band is off.'
            | i18n
        }}
        {{
          (context.data.otherBand
            ? 'Join from a wired device or from the other band.'
            : 'Join from a wired device.'
          ) | i18n
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
      minBytes: 'Must be at least 8 characters',
      maxBytes: 'Too long',
      control: 'Must not contain control characters',
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
    TuiValueChanges,
    i18nPipe,
  ],
})
class WwanJoinDialog {
  private readonly i18n = inject(i18nPipe)

  protected readonly context =
    injectContext<TuiDialogContext<WwanConnectRequest, WwanJoinData>>()
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
      [Validators.required, byteLength(1, 32), noControlCharacters],
    ],
    security: [this.network?.security ?? ('wpa2' as WwanSecurity)],
    password: [
      { value: '', disabled: this.network?.security === 'open' },
      [Validators.required, byteLength(8, 63), noControlCharacters],
    ],
    priority: [this.context.data.priority ?? ('backup' as WwanPriority)],
  })

  protected readonly stringifySecurity = (s: WwanSecurity): string =>
    securityLabel(this.i18n, s)

  protected readonly priority = ({ $implicit }: TuiContext<WwanPriority>) =>
    this.i18n.transform(PRIORITY_LABELS[$implicit])

  protected onSecurity(security: WwanSecurity): void {
    const { password } = this.form.controls
    security === 'open' ? password.disable() : password.enable()
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
