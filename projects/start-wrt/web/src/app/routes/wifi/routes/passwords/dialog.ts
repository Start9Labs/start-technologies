import { Component, inject, signal } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { RouterLink } from '@angular/router'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiAppearance,
  TuiButton,
  TuiDataList,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { TuiChevron, TuiPassword, TuiSelect } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { ApiService } from 'src/app/services/api/api.service'
import type { ProfileId } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

export interface WifiPasswordEntry {
  label: string
  password: string
  profile: string
}

// @TODO matt review
export interface WifiPasswordDialogData {
  profiles: ProfileId[]
  entry?: WifiPasswordEntry
}

// @TODO matt review
export interface WifiPasswordDialogResult {
  label: string
  password?: string
  profile: ProfileId | null
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <tui-textfield>
        <label tuiLabel>{{ 'Label' | i18n }}</label>
        <input
          tuiInput
          [placeholder]="'Name for this Wi-Fi network' | i18n"
          formControlName="label"
        />
      </tui-textfield>
      <tui-error formControlName="label" />
      @if (!editing) {
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input tuiInput formControlName="password" type="password" />
          <tui-icon tuiPassword />
          <button
            tuiIconButton
            type="button"
            iconStart="@tui.refresh-cw"
            size="s"
            (click)="generatePassword()"
          >
            {{ 'Generate' | i18n }}
          </button>
        </tui-textfield>
        <tui-error formControlName="password" />
      }
      <tui-textfield tuiChevron>
        <label tuiLabel>{{ 'Security Profile' | i18n }}</label>
        <input tuiSelect formControlName="profile" />
        <tui-data-list *tuiDropdown>
          <tui-opt-group>
            <button tuiOption value="Admin">{{ 'Admin' | i18n }}</button>
            @for (item of profiles; track item.vlan_tag) {
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
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideHelp('/wifi/passwords/dialog'),
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiError,
    TuiButton,
    TuiInput,
    TuiIcon,
    TuiPassword,
    TuiSelect,
    TuiChevron,
    TuiDataList,
    TuiAppearance,
    RouterLink,
    i18nPipe,
  ],
})
class AddWifiPassword {
  protected readonly context =
    injectContext<
      TuiDialogContext<WifiPasswordDialogResult, WifiPasswordDialogData>
    >()

  private readonly api = inject(ApiService)
  private readonly entry = this.context.data.entry
  protected readonly editing = !!this.entry
  protected readonly profiles = this.context.data.profiles

  protected readonly form = inject(NonNullableFormBuilder).group({
    label: [this.entry?.label ?? '', Validators.required],
    password: [
      this.entry?.password ?? '',
      [Validators.required, Validators.minLength(8)],
    ],
    profile: [this.entry?.profile ?? '', Validators.required],
  })

  constructor() {
    if (!this.entry) {
      this.api.wifiGeneratePassword().then(password => {
        this.form.controls.password.setValue(password)
      })
    }
  }

  protected async generatePassword(): Promise<void> {
    this.form.controls.password.setValue(await this.api.wifiGeneratePassword())
  }

  protected save(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      const { label, password, profile } = this.form.getRawValue()
      // @TODO matt review
      const resolved = this.profiles.find(p => p.fullname === profile) ?? null
      this.context.completeWith({
        label,
        profile: resolved,
        password: this.editing ? undefined : password,
      })
    }
  }
}

export const ADD_WIFI_PASSWORD = new PolymorpheusComponent(AddWifiPassword)
