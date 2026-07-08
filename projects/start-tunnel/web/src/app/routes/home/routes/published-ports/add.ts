import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { TaskService } from '@start9labs/shared'
import { TuiContext, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiNumberFormat,
} from '@taiga-ui/core'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiInputNumber,
  TuiRadioList,
  TuiSelect,
  TuiTooltip,
} from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nKey } from 'src/app/i18n/i18n.providers'
import { ApiService } from 'src/app/services/api/api.service'

import { MappedDevice, PublishedPortsData } from './utils'

// A range counts up from both the external and internal port, so neither side
// may run past the u16 port space. Mirrors the server-side guard in add_forward.
function portRangeOverflow(group: AbstractControl): ValidationErrors | null {
  const ext = group.get('externalport')?.value
  const int = group.get('internalport')?.value
  const count = group.get('count')?.value ?? 1
  if (count <= 1) return null
  const last = count - 1
  return (ext != null && ext + last > 65535) ||
    (int != null && int + last > 65535)
    ? { portRangeOverflow: true }
    : null
}

// IPv6 needs the selected server to have a GUA (its subnet needs a v6 prefix).
function ipVersionRequiresGua(group: AbstractControl): ValidationErrors | null {
  const ipVersion = group.get('ipVersion')?.value
  const device = group.get('device')?.value as MappedDevice | null
  const needsV6 = ipVersion === 'ipv6' || ipVersion === 'both'
  return needsV6 && device && !device.ipv6 ? { noGua: true } : null
}

const IP_VERSION: Record<string, i18nKey> = {
  ipv4: 'IPv4',
  ipv6: 'IPv6',
  both: 'IPv4 + IPv6',
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form">
      <tui-textfield>
        <label tuiLabel>{{ 'Label' | i18n }}</label>
        <input tuiInput formControlName="label" />
      </tui-textfield>
      <tui-error formControlName="label" />
      <tui-textfield>
        <label tuiLabel>{{ 'External Port' | i18n }}</label>
        <input
          tuiInputNumber
          formControlName="externalport"
          [min]="0"
          [max]="65535"
          [tuiNumberFormat]="{ thousandSeparator: '' }"
        />
      </tui-textfield>
      <tui-error formControlName="externalport" />
      <tui-textfield
        tuiChevron
        [stringify]="stringify"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Server' | i18n }}</label>
        @if (mobile) {
          <select
            tuiSelect
            formControlName="device"
            [placeholder]="'Select Server' | i18n"
            [items]="context.data.devices()"
          ></select>
        } @else {
          <input tuiSelect formControlName="device" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper
            *tuiDropdown
            [items]="context.data.devices()"
          />
        }
      </tui-textfield>
      <tui-error formControlName="device" />
      <tui-textfield>
        <label tuiLabel>{{ 'Internal Port' | i18n }}</label>
        <input
          tuiInputNumber
          formControlName="internalport"
          [min]="0"
          [max]="65535"
          [tuiNumberFormat]="{ thousandSeparator: '' }"
        />
      </tui-textfield>
      <tui-error formControlName="internalport" />
      <tui-textfield>
        <label tuiLabel>{{ 'Number of Ports' | i18n }}</label>
        <input
          tuiInputNumber
          formControlName="count"
          [min]="1"
          [max]="65535"
          [tuiNumberFormat]="{ thousandSeparator: '' }"
        />
        <tui-icon [tuiTooltip]="countHint | i18n" />
      </tui-textfield>
      <tui-error formControlName="count" />
      @if (form.errors?.['portRangeOverflow']) {
        <tui-error
          [error]="'Port range runs past the maximum port (65535)' | i18n"
        />
      }

      <fieldset>
        <legend>{{ 'IP Version' | i18n }}</legend>
        <tui-radio-list
          size="s"
          formControlName="ipVersion"
          [items]="ipVersionValues"
          [itemContent]="ipVersionLabel"
        />
      </fieldset>
      @if (guaError()) {
        <tui-error
          [error]="
            'Selected server has no IPv6 address — its subnet needs an IPv6 prefix'
              | i18n
          "
        />
      }

      @if (form.value.ipVersion !== 'ipv6' && !isRange) {
        <tui-textfield>
          <label tuiLabel>
            {{
              (form.value.ipVersion === 'both'
                ? 'Hostname (optional, ipv4 only)'
                : 'Hostname (optional)'
              ) | i18n
            }}
          </label>
          <input
            tuiInput
            formControlName="sni"
            placeholder="host.example.com"
          />
          <tui-icon [tuiTooltip]="hostnameHint | i18n" />
        </tui-textfield>
      }
      <footer>
        <button tuiButton [disabled]="form.invalid" (click)="onSave()">
          {{ 'Save' | i18n }}
        </button>
      </footer>
    </form>
  `,
  styles: `
    tui-radio-list {
      flex-direction: row;
    }
  `,
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiChevron,
    TuiDataListWrapper,
    TuiError,
    TuiInputNumber,
    TuiNumberFormat,
    TuiRadioList,
    TuiSelect,
    TuiForm,
    TuiTooltip,
    TuiIcon,
    TuiInput,
    i18nPipe,
  ],
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/published-ports/add')],
})
export class PublishedPortsAdd {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)

  protected readonly hostnameHint =
    'Only supported for SSL/TLS services — the gateway routes by the TLS SNI, so several hostnames can share one external port. IPv4 only. Leave blank for a plain published port.'

  protected readonly countHint =
    'Publish this many consecutive ports, counting up from the external and internal ports above. Leave at 1 for a single port. SNI hostnames are not supported for ranges.'

  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly context =
    injectContext<TuiDialogContext<void, PublishedPortsData>>()

  protected readonly ipVersionValues = ['ipv4', 'ipv6', 'both'] as const
  protected readonly ipVersionLabel = (ctx: TuiContext<string>) =>
    this.i18n.transform(IP_VERSION[ctx.$implicit])

  protected readonly form = inject(NonNullableFormBuilder).group(
    {
      label: ['', Validators.required],
      externalport: [null as number | null, Validators.required],
      device: [null as MappedDevice | null, Validators.required],
      internalport: [null as number | null, Validators.required],
      ipVersion: ['ipv4' as 'ipv4' | 'ipv6' | 'both'],
      sni: [''],
      count: [
        1,
        [Validators.required, Validators.min(1), Validators.max(65535)],
      ],
    },
    { validators: [portRangeOverflow, ipVersionRequiresGua] },
  )

  private readonly selectedDevice = toSignal(
    this.form.controls.device.valueChanges,
    { initialValue: null },
  )
  private readonly selectedVersion = toSignal(
    this.form.controls.ipVersion.valueChanges,
    { initialValue: 'ipv4' as const },
  )

  // The v6-requires-GUA error, shown proactively (mirrors the form validator).
  protected readonly guaError = computed(() => {
    const v = this.selectedVersion()
    const device = this.selectedDevice()
    return (v === 'ipv6' || v === 'both') && !!device && !device.ipv6
  })

  protected get isRange(): boolean {
    return this.form.controls.count.value > 1
  }

  protected readonly stringify = ({ ip, name }: MappedDevice) =>
    ip ? `${name} (${ip})` : ''

  protected async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)

      return
    }

    const { label, externalport, device, internalport, sni, count, ipVersion } =
      this.form.getRawValue()

    const isRange = count > 1
    // SNI demux is IPv4-only and per-port; it applies to the v4 side even in
    // "both" mode (the v6 side is always a plain pinhole). Ignored for v6-only
    // and for ranges.
    const hostname = isRange || ipVersion === 'ipv6' ? '' : sni.trim()
    const v4 = ipVersion === 'ipv4' || ipVersion === 'both'
    const v6 = ipVersion === 'ipv6' || ipVersion === 'both'

    this.tasks.run(async () => {
      if (v4) {
        // The external IP is fixed server-side to the target device's WAN.
        await this.api.addForward({
          externalPort: externalport!,
          target: `${device!.ip}:${internalport}`,
          label,
          sni: hostname ? [hostname] : [],
          count,
        })
      }
      if (v6 && device!.ipv6) {
        await this.api.addPinhole({
          gua: device!.ipv6,
          externalPort: externalport!,
          internalPort:
            internalport !== externalport ? internalport! : undefined,
          label,
          count,
        })
      }
      this.context.$implicit.complete()
    })
  }
}

export const PUBLISHED_PORTS_ADD = new PolymorpheusComponent(PublishedPortsAdd)
