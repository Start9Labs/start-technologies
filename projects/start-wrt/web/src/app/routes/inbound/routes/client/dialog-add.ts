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
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'

export interface ClientDialogData {
  /** Without one, the router picks the address. */
  peerSubnet?: string | null
  peers: readonly VpnServerPeer[]
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
      @if (prefix) {
        <tui-textfield>
          <label tuiLabel>{{ 'IP Address' | i18n }}</label>
          <input tuiInput formControlName="ip" />
        </tui-textfield>
        <tui-error formControlName="ip" />
      }
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
    provideTranslatedValidationErrors({
      required: 'Required',
      ipv4: 'Enter a valid IPv4 address',
      outsideProfile: 'Outside the subnet of this VPN',
      notHost: 'Not a usable host address',
      uniqueIp: 'Already used by another client of this VPN',
    }),
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

  protected readonly prefix = this.context.data.peerSubnet?.replace(
    /\.0\/24$/,
    '',
  )
  // Unique across subnets: each is also the peer's IPv6 host id.
  private readonly usedHosts = this.context.data.peers
    .map(peer => Number(peer.ip?.split('.')[3]))
    .filter(Number.isInteger)

  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', Validators.required],
    ip: [
      this.prefix ? this.nextAvailableIp(this.prefix) : '',
      this.prefix
        ? [
            Validators.required,
            CustomValidators.ipv4(),
            CustomValidators.ipv4HostIn(`${this.prefix}.1/24`),
            this.uniqueHost(),
          ]
        : [],
    ],
    public_key: [''],
    route_all: [false],
  })

  private nextAvailableIp(prefix: string): string {
    const host = [...Array(253).keys()]
      .map(i => i + 2)
      .find(host => !this.usedHosts.includes(host))
    return host ? `${prefix}.${host}` : ''
  }

  private uniqueHost(): ValidatorFn {
    return (control: AbstractControl) => {
      const host = Number(control.value?.split('.')[3])
      return this.usedHosts.includes(host) ? { uniqueIp: true } : null
    }
  }

  protected save(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      const { name, ip, public_key, route_all } = this.form.getRawValue()
      this.context.completeWith({
        name,
        ip: ip || undefined,
        public_key: public_key || undefined,
        route_all: route_all || undefined,
      })
    }
  }
}

export const ADD_CLIENT = new PolymorpheusComponent(AddClient)
