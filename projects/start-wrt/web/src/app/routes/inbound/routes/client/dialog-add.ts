import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { CustomValidators } from 'src/app/utils/validators'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { VpnServerPeer } from 'src/app/routes/inbound/service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

export interface ClientDialogData {
  serverAddress: string
  usedIps: string[]
  defaults?: {
    name?: string
    ip?: string
    route_all?: boolean
  }
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <tui-textfield>
        <label tuiLabel>{{ 'Label' | i18n }}</label>
        <input
          tuiInput
          [placeholder]="'What to call the client device' | i18n"
          formControlName="name"
        />
      </tui-textfield>
      <tui-error formControlName="name" />
      <tui-textfield>
        <label tuiLabel>{{ 'LAN IP Address' | i18n }}</label>
        <input tuiInput formControlName="ip" />
      </tui-textfield>
      <tui-error formControlName="ip" />
      <tui-textfield>
        <label tuiLabel>{{ 'Public Key (optional)' | i18n }}</label>
        <input tuiInput formControlName="public_key" />
      </tui-textfield>
      <label tuiLabel>
        <input type="checkbox" tuiSwitch formControlName="route_all" />
        {{ 'Route all traffic through tunnel' | i18n }}
      </label>
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
    provideHelp('/inbound/client/dialog-add'),
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiError,
    TuiButton,
    TuiInput,
    TuiSwitch,
    i18nPipe,
  ],
})
class AddClient {
  protected readonly context =
    injectContext<TuiDialogContext<VpnServerPeer, ClientDialogData>>()

  private readonly serverAddress = this.context.data.serverAddress
  private readonly usedIps = this.context.data.usedIps
  private readonly defaults = this.context.data.defaults

  protected readonly form = inject(NonNullableFormBuilder).group({
    name: [this.defaults?.name ?? '', Validators.required],
    ip: [
      this.defaults?.ip ?? this.nextAvailableIp(),
      [
        Validators.required,
        CustomValidators.ipv4(),
        this.peerIpRange(),
        this.uniqueIp(),
      ],
    ],
    public_key: [''],
    route_all: [this.defaults?.route_all ?? false],
  })

  private nextAvailableIp(): string {
    const prefix = this.serverAddress.replace(/\.\d+$/, '')
    for (let i = 200; i <= 253; i++) {
      const candidate = `${prefix}.${i}`
      if (!this.usedIps.includes(candidate)) return candidate
    }
    return `${prefix}.200`
  }

  private peerIpRange(): ValidatorFn {
    return (control: AbstractControl) => {
      if (!control.value) return null
      const prefix = this.serverAddress.replace(/\.\d+$/, '')
      const match = control.value.match(/^(.+)\.(\d+)$/)
      if (!match) return { peerIpRange: true }
      const octet = parseInt(match[2], 10)
      if (match[1] !== prefix || octet < 200 || octet > 253) {
        return { peerIpRange: true }
      }
      return null
    }
  }

  private uniqueIp(): ValidatorFn {
    return (control: AbstractControl) => {
      if (!control.value) return null
      return this.usedIps.includes(control.value) ? { uniqueIp: true } : null
    }
  }

  protected save(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      const { name, ip, public_key, route_all } = this.form.getRawValue()
      this.context.completeWith({
        name,
        ip,
        public_key: public_key || undefined,
        route_all: route_all || undefined,
      })
    }
  }
}

export const ADD_CLIENT = new PolymorpheusComponent(AddClient)
