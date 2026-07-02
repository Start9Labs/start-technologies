import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { RouterLink } from '@angular/router'
import { TuiAutoFocus, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiAppearance,
  TuiButton,
  TuiDataList,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiNumberFormat,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { TuiChevron, TuiInputNumber, TuiSelect } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { ProfileId, VpnServerEndpoint } from 'src/app/services/api/api.service'
import { VpnServer } from 'src/app/routes/inbound/service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  provideTranslatedValidationErrors,
  tpl,
} from 'src/app/i18n/validation-errors'

export interface ServerDialogData {
  server?: VpnServer
  profiles: ProfileId[]
  endpoints: VpnServerEndpoint[]
  usedPorts: number[]
}

export interface ServerDialogResult {
  profile: string
  label: string
  enabled: boolean
  listen_port: number
  endpoint: string
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <tui-textfield>
        <label tuiLabel>{{ 'Label' | i18n }}</label>
        <input
          tuiInput
          tuiAutoFocus
          [placeholder]="'What to call this VPN connection' | i18n"
          formControlName="label"
        />
      </tui-textfield>
      <tui-error formControlName="label" />
      <tui-textfield tuiChevron>
        <label tuiLabel>{{ 'Endpoint' | i18n }}</label>
        <input tuiSelect formControlName="endpoint" />
        <tui-data-list *tuiDropdown>
          @for (item of endpoints; track $index) {
            <button tuiOption [value]="item.address">
              {{ item.address }}
              <span class="g-secondary">({{ item.label }})</span>
            </button>
          }
        </tui-data-list>
      </tui-textfield>
      <tui-error formControlName="endpoint" />
      <tui-textfield tuiChevron>
        <label tuiLabel>{{ 'Security Profile' | i18n }}</label>
        <input tuiSelect formControlName="profile" />
        <tui-data-list *tuiDropdown>
          <tui-opt-group>
            @for (item of profiles; track $index) {
              <button tuiOption [value]="item.fullname">
                {{ item.fullname }}
              </button>
            }
          </tui-opt-group>
          <tui-opt-group>
            <a
              tuiOption
              tuiAppearance="action"
              routerLink="profiles"
              iconStart="@tui.user-lock"
              (click)="context.$implicit.complete()"
            >
              {{ 'Manage profiles' | i18n }}
            </a>
          </tui-opt-group>
        </tui-data-list>
      </tui-textfield>
      <tui-error formControlName="profile" />
      <tui-textfield
        [tuiNumberFormat]="{ precision: 0, thousandSeparator: '' }"
      >
        <label tuiLabel>{{ 'Port' | i18n }}</label>
        <input tuiInputNumber formControlName="listen_port" />
      </tui-textfield>
      <tui-error formControlName="listen_port" />
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>
          {{ (context.data.server ? 'Save VPN' : 'Add VPN') | i18n }}
        </button>
      </footer>
    </form>
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideHelp('/inbound/dialog'),
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
    provideTranslatedValidationErrors({
      required: 'This field is required',
      min: tpl<{ min: number }>('Minimum value is {min}', ({ min }) => ({
        min,
      })),
      max: tpl<{ max: number }>('Maximum value is {max}', ({ max }) => ({
        max,
      })),
      uniquePort: 'Port is already in use',
    }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiError,
    TuiButton,
    TuiInput,
    TuiSelect,
    TuiChevron,
    TuiDataList,
    RouterLink,
    TuiAppearance,
    TuiInputNumber,
    TuiNumberFormat,
    TuiAutoFocus,
    i18nPipe,
  ],
})
class AddServer {
  protected readonly context =
    injectContext<TuiDialogContext<ServerDialogResult, ServerDialogData>>()

  private readonly server = this.context.data?.server
  private readonly usedPorts = this.context.data?.usedPorts ?? []
  protected readonly endpoints = this.context.data?.endpoints ?? []
  protected readonly profiles = this.context.data?.profiles ?? []

  private readonly profileToInterface: Record<string, string> =
    Object.fromEntries(this.profiles.map(p => [p.fullname, p.interface]))

  protected readonly form = inject(NonNullableFormBuilder).group({
    label: [this.server?.label ?? '', Validators.required],
    endpoint: [this.server?.endpoint ?? '', Validators.required],
    listen_port: [
      this.server?.listen_port ?? this.nextAvailablePort(),
      [
        Validators.required,
        Validators.min(1),
        Validators.max(65535),
        this.uniquePortValidator(),
      ],
    ],
    profile: [
      this.profiles.find(p => p.interface === this.server?.profile)?.fullname ??
        '',
      Validators.required,
    ],
  })

  private nextAvailablePort(): number {
    let port = 51820
    while (this.usedPorts.includes(port)) port++
    return port
  }

  private uniquePortValidator(): (
    control: AbstractControl,
  ) => ValidationErrors | null {
    return (control: AbstractControl) => {
      return this.usedPorts.includes(control.value)
        ? { uniquePort: true }
        : null
    }
  }

  protected save(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      const raw = this.form.getRawValue()
      this.context.completeWith({
        ...raw,
        profile: this.profileToInterface[raw.profile],
        enabled: this.server?.enabled ?? true,
      })
    }
  }
}

export const ADD_SERVER = new PolymorpheusComponent(AddServer)
