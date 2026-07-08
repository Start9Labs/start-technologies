import { Component, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiAutoFocus, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCheckbox,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
} from '@taiga-ui/core'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiSelect,
  TuiTooltip,
} from '@taiga-ui/kit'
import { TuiElasticContainer, TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import {
  matchWan,
  toWanItems,
  WanItem,
  wanLabel,
} from 'src/app/routes/home/components/wan'
import { ApiService } from 'src/app/services/api/api.service'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

import { DEVICES_CONFIG } from './config'
import {
  DeviceData,
  getIp,
  ipInSubnetValidator,
  MappedSubnet,
  subnetValidator,
} from './utils'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form">
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input tuiInput tuiAutoFocus formControlName="name" />
      </tui-textfield>
      <tui-error formControlName="name" />

      @if (!context.data.device) {
        <tui-textfield
          tuiChevron
          [stringify]="stringify"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Subnet' | i18n }}</label>
          @if (mobile) {
            <select
              tuiSelect
              formControlName="subnet"
              [placeholder]="'Select Subnet' | i18n"
              [items]="context.data.subnets()"
            ></select>
          } @else {
            <input tuiSelect formControlName="subnet" />
          }
          @if (!mobile) {
            <tui-data-list-wrapper
              *tuiDropdown
              [items]="context.data.subnets()"
              (itemClick)="onSubnet($event)"
            />
          }
        </tui-textfield>
        <tui-error formControlName="subnet" />

        <tui-elastic-container>
          @if (form.controls.subnet.value?.range) {
            <tui-textfield>
              <label tuiLabel>{{ 'LAN IP' | i18n }}</label>
              <input tuiInput tuiAutoFocus formControlName="ip" />
            </tui-textfield>
          }
        </tui-elastic-container>
        @if (form.controls.subnet.value?.range) {
          <tui-error formControlName="ip" />
        }
      }

      <tui-textfield
        tuiChevron
        [identityMatcher]="matchWan"
        [stringify]="stringifyWan"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'WAN IP' | i18n }}</label>
        @if (mobile) {
          <select tuiSelect formControlName="wanIp" [items]="wanItems"></select>
        } @else {
          <input tuiSelect formControlName="wanIp" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper *tuiDropdown [items]="wanItems" />
        }
      </tui-textfield>

      <tui-elastic-container>
        @if (!context.data.device && kind === 'server') {
          <label tuiLabel>
            <input
              id="dnsInjectionHint"
              tuiCheckbox
              type="checkbox"
              formControlName="dnsInjection"
            />
            {{ 'Allow DNS Injection' | i18n }}
            <tui-icon
              tuiTooltipDescribe="dnsInjectionHint"
              [tuiTooltip]="dnsInjectionHint"
            />
          </label>
          <label tuiLabel>
            <input
              id="autoPortForward"
              tuiCheckbox
              type="checkbox"
              formControlName="autoPortForward"
            />
            {{ 'Allow auto-publish' | i18n }}
            <tui-icon
              tuiTooltipDescribe="autoPortForward"
              [tuiTooltip]="autoPortForwardHint"
            />
          </label>
        }
      </tui-elastic-container>

      <footer>
        <button tuiButton (click)="onSave()">{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  imports: [
    ReactiveFormsModule,
    TuiAutoFocus,
    TuiButton,
    TuiCheckbox,
    TuiDataListWrapper,
    TuiError,
    TuiForm,
    TuiTooltip,
    TuiIcon,
    TuiSelect,
    TuiInput,
    TuiChevron,
    TuiElasticContainer,
    i18nPipe,
  ],
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/devices/add')],
})
export class DevicesAdd {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)

  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly context =
    injectContext<TuiDialogContext<void, DeviceData>>()

  private readonly fb = inject(NonNullableFormBuilder)
  private readonly i18n = inject(i18nPipe)

  private readonly autoSubnet =
    !this.context.data.device && this.context.data.subnets().length === 1
      ? this.context.data.subnets().at(0)
      : undefined

  protected readonly form = this.fb.group({
    name: [this.context.data.device?.name || '', Validators.required],
    subnet: [
      this.context.data.device?.subnet ?? this.autoSubnet,
      [Validators.required, subnetValidator(this.i18n)],
    ],
    ip: [
      this.context.data.device?.ip ||
        (this.autoSubnet ? getIp(this.autoSubnet) : ''),
      this.autoSubnet
        ? [
            Validators.required,
            ipInSubnetValidator(this.i18n, this.autoSubnet.range),
          ]
        : [],
    ],
    wanIp: this.fb.control<WanItem>({
      ip: this.context.data.device?.wanIp ?? null,
    }),
    dnsInjection: [this.context.data.device?.allowDnsInjection ?? true],
    autoPortForward: [this.context.data.device?.allowAutoPortForward ?? true],
  })

  // Inferred from which "Add" button opened the dialog, not user-selectable.
  protected readonly kind: T.Tunnel.WgClientKind =
    this.context.data.kind ?? this.context.data.device?.kind ?? 'client'

  protected readonly dnsInjectionHint = this.i18n.transform(
    'The device can add/update the DNS records the tunnel serves for every peer to resolve. Only enable for devices you trust.',
  )
  protected readonly autoPortForwardHint = this.i18n.transform(
    'The device can publish its own ports on the gateway automatically (via PCP). Only enable for devices you trust.',
  )

  protected readonly wanItems = toWanItems(this.context.data.wanOptions)

  protected readonly stringify = ({ range, name }: MappedSubnet) =>
    range ? `${name} (${range})` : ''
  protected readonly stringifyWan = ({ ip }: WanItem) =>
    wanLabel(ip, this.i18n.transform('Subnet default'), this.subnetWanIp())
  protected readonly matchWan = matchWan

  // The address the device inherits on "Subnet default": the selected subnet's
  // own WAN override, or the system default when the subnet has none.
  private subnetWanIp(): string | null {
    const range = this.form.controls.subnet.value?.range
    return (
      this.context.data.subnets().find(s => s.range === range)?.wanIp ??
      this.context.data.defaultWan
    )
  }

  protected onSubnet(subnet: MappedSubnet) {
    this.form.controls.ip.clearValidators()
    this.form.controls.ip.addValidators([
      Validators.required,
      ipInSubnetValidator(this.i18n, subnet.range),
    ])
    const ip = getIp(subnet)

    if (ip) {
      this.form.controls.ip.setValue(ip)
    } else {
      this.form.controls.ip.disable()
    }

    this.form.controls.subnet.markAsTouched()
  }

  protected async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)

      return
    }

    const { ip, name, subnet, wanIp, dnsInjection, autoPortForward } =
      this.form.getRawValue()
    const data = { ip, name, subnet: subnet?.range || '' }
    const device = this.context.data.device
    const kind = this.kind

    this.tasks.run(async () => {
      if (device) {
        await this.api.editDevice({ ...data, kind: device.kind })
      } else {
        await this.api.addDevice({ ...data, kind })
      }

      if (wanIp.ip !== (device?.wanIp ?? null)) {
        await this.api.setDeviceWan({
          subnet: data.subnet,
          ip,
          wanIp: wanIp.ip,
        })
      }

      // addDevice sets both flags on for a server; only sync the ones unchecked.
      if (!device && kind === 'server') {
        if (!dnsInjection) {
          await this.api.setDnsInjection({
            subnet: data.subnet,
            ip,
            enabled: false,
          })
        }
        if (!autoPortForward) {
          await this.api.setAutoPortForward({
            subnet: data.subnet,
            ip,
            enabled: false,
          })
        }
      }

      if (!device) {
        const config = await this.api.showDeviceConfig({
          subnet: data.subnet,
          ip,
        })

        this.dialogs
          .open(DEVICES_CONFIG, { data: config, closable: false, size: 'm' })
          .subscribe()
      }
      this.context.$implicit.complete()
    })
  }
}

export const DEVICES_ADD = new PolymorpheusComponent(DevicesAdd)
