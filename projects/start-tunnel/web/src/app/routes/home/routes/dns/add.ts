import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  Signal,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { TaskService } from '@start9labs/shared'
import { utils } from '@start9labs/start-core'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiNumberFormat,
} from '@taiga-ui/core'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiInputNumber,
  TuiSelect,
} from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { MappedDevice } from 'src/app/routes/home/routes/published-ports/utils'
import { ApiService } from 'src/app/services/api/api.service'

const TYPES = ['A', 'AAAA', 'CNAME', 'TXT'] as const

interface DnsAddData {
  readonly devices: Signal<readonly MappedDevice[]>
}

// Sentinel "device" for manual entry; ip === '' marks the Other branch.
const OTHER: MappedDevice = { ip: '', name: 'Other (custom)' }

function ipValidator(v6: boolean): ValidatorFn {
  return ({ value }: AbstractControl): ValidationErrors | null => {
    if (!value) return null
    try {
      const net = utils.IpNet.parse(`${value}/${v6 ? 128 : 32}`)
      return (v6 ? net.isIpv6() : net.isIpv4()) ? null : { ip: true }
    } catch {
      return { ip: true }
    }
  }
}

function configure(
  ctrl: AbstractControl,
  on: boolean,
  validators: ValidatorFn[],
) {
  if (on) {
    ctrl.setValidators(validators)
    ctrl.enable({ emitEvent: false })
  } else {
    ctrl.clearValidators()
    ctrl.disable({ emitEvent: false })
  }
  ctrl.updateValueAndValidity({ emitEvent: false })
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form">
      <tui-textfield>
        <label tuiLabel>{{ 'Hostname' | i18n }}</label>
        <input tuiInput formControlName="name" placeholder="host.example.com" />
      </tui-textfield>
      <tui-error formControlName="name" />
      <tui-textfield tuiChevron [tuiTextfieldCleaner]="false">
        <label tuiLabel>{{ 'Type' | i18n }}</label>
        @if (mobile) {
          <select tuiSelect formControlName="type" [items]="types"></select>
        } @else {
          <input tuiSelect formControlName="type" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper *tuiDropdown [items]="types" />
        }
      </tui-textfield>
      <tui-error formControlName="type" />
      @if (isAddr()) {
        <tui-textfield
          tuiChevron
          [stringify]="stringifyDevice"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Server' | i18n }}</label>
          @if (mobile) {
            <select
              tuiSelect
              formControlName="device"
              [items]="deviceItems()"
              [placeholder]="'Select server' | i18n"
            ></select>
          } @else {
            <input
              tuiSelect
              formControlName="device"
              [placeholder]="'Select server' | i18n"
            />
          }
          @if (!mobile) {
            <tui-data-list-wrapper *tuiDropdown [items]="deviceItems()" />
          }
        </tui-textfield>
        <tui-error formControlName="device" />
        @if (isOther()) {
          <tui-textfield>
            <label tuiLabel>
              {{
                type() === 'AAAA'
                  ? ('IPv6 address' | i18n)
                  : ('IPv4 address' | i18n)
              }}
            </label>
            <input
              tuiInput
              formControlName="custom"
              [placeholder]="type() === 'AAAA' ? '2001:db8::1' : '192.0.2.10'"
            />
          </tui-textfield>
          <tui-error formControlName="custom" />
        }
      } @else {
        <tui-textfield>
          <label tuiLabel>{{ 'Value' | i18n }}</label>
          <input tuiInput formControlName="value" />
        </tui-textfield>
        <tui-error formControlName="value" />
      }
      <tui-textfield>
        <label tuiLabel>{{ 'TTL (seconds)' | i18n }}</label>
        <input
          tuiInputNumber
          formControlName="ttl"
          [min]="0"
          [tuiNumberFormat]="{ thousandSeparator: '' }"
        />
      </tui-textfield>
      <footer>
        <button tuiButton (click)="onSave()">{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/dns/add')],
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiChevron,
    TuiDataListWrapper,
    TuiError,
    TuiInput,
    TuiInputNumber,
    TuiNumberFormat,
    TuiSelect,
    TuiForm,
    i18nPipe,
  ],
})
export class DnsAdd {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)

  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly context =
    injectContext<TuiDialogContext<void, DnsAddData>>()
  protected readonly types = TYPES

  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', Validators.required],
    type: ['A' as (typeof TYPES)[number], Validators.required],
    device: [null as MappedDevice | null],
    custom: [''],
    value: [''],
    ttl: [300 as number | null],
  })

  protected readonly type = toSignal(this.form.controls.type.valueChanges, {
    initialValue: this.form.controls.type.value,
  })
  private readonly device = toSignal(this.form.controls.device.valueChanges, {
    initialValue: this.form.controls.device.value,
  })

  protected readonly isAddr = computed(
    () => this.type() === 'A' || this.type() === 'AAAA',
  )
  protected readonly isOther = computed(
    () => this.isAddr() && this.device()?.ip === '',
  )
  protected readonly deviceItems = computed(() => [
    ...this.context.data.devices(),
    { ...OTHER, name: this.i18n.transform('Other (custom)') },
  ])

  protected readonly stringifyDevice = ({ name, ip }: MappedDevice) =>
    ip ? `${name} (${ip})` : name

  // Keep only the controls relevant to the current type/branch enabled, so
  // form.invalid reflects exactly what the user must fill in.
  private readonly reconcile = effect(() => {
    const c = this.form.controls
    const addr = this.isAddr()
    configure(c.device, addr, [Validators.required])
    configure(c.custom, this.isOther(), [
      Validators.required,
      ipValidator(this.type() === 'AAAA'),
    ])
    configure(c.value, !addr, [Validators.required])
  })

  protected async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      return
    }

    const { name, type, device, custom, value, ttl } = this.form.getRawValue()
    const finalValue =
      type === 'A' || type === 'AAAA'
        ? device?.ip === ''
          ? custom.trim()
          : (device?.ip ?? '')
        : value.trim()

    this.tasks.run(async () => {
      await this.api.addDnsRecord({ name, type, value: finalValue, ttl })
      this.context.$implicit.complete()
    })
  }
}

export const DNS_ADD = new PolymorpheusComponent(DnsAdd)
