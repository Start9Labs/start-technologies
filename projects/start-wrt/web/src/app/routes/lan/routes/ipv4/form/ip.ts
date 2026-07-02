import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ReactiveFormsModule } from '@angular/forms'
import { tuiControlValue, TuiValidator } from '@taiga-ui/cdk'
import {
  TuiDropdown,
  TuiError,
  TuiGroup,
  TuiInput,
  TuiTextfield,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiDataListWrapper, TuiInputNumber, TuiSelect } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm } from '@taiga-ui/layout'
import { FORM } from 'src/app/components/form'
import LanIpv4 from '../'
import {
  FIRST_OCTETS,
  FirstOctet,
  getSecondOctetRange,
  isSecondOctetInRange,
  LAN_IPV4_VALIDATION_ERRORS,
  resolveSecondOctet,
  secondOctetBlockValidator,
} from '../utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: 'lan-ipv4-ip',
  template: `
    <fieldset>
      <legend>{{ 'Network Block' | i18n }}</legend>
      <div tuiGroup>
        <tui-textfield tuiDropdownLimitWidth="min">
          <input tuiSelect formControlName="firstOctet" />
          <tui-data-list-wrapper *tuiDropdown [items]="firstOctets" />
        </tui-textfield>
        @if (secondOctetFixed()) {
          <tui-textfield>
            <input tuiInput [value]="secondOctet()" disabled />
          </tui-textfield>
        } @else {
          <tui-textfield>
            <input
              tuiInputNumber
              formControlName="secondOctet"
              [tuiValidator]="validator()"
            />
          </tui-textfield>
        }
        <tui-textfield>
          <input tuiInput value="x" disabled />
        </tui-textfield>
        <tui-textfield>
          <input tuiInput value="x" disabled />
        </tui-textfield>
        /16
      </div>
    </fieldset>
    <tui-error [error]="secondOctetError()" />
  `,
  styles: `
    tui-textfield {
      max-inline-size: 3.25rem;
    }

    div {
      align-items: center;
      text-indent: 0.5rem;
    }
  `,
  viewProviders: [FORM],
  hostDirectives: [TuiForm, TuiCardLarge],
  providers: [provideTranslatedValidationErrors(LAN_IPV4_VALIDATION_ERRORS)],
  imports: [
    ReactiveFormsModule,
    TuiTextfield,
    TuiError,
    TuiInputNumber,
    TuiSelect,
    TuiDataListWrapper,
    TuiInput,
    TuiGroup,
    TuiDropdown,
    TuiValidator,
    i18nPipe,
  ],
})
export class LanIpv4Ip {
  protected readonly parent = inject(LanIpv4)
  protected readonly firstOctets = FIRST_OCTETS

  private readonly ip = this.parent.form.controls.ip

  readonly firstOctet = toSignal(
    tuiControlValue<FirstOctet>(this.ip.controls.firstOctet),
    { requireSync: true },
  )

  // The 192 block's octet is fixed, so the control can carry a stale value from
  // a previously selected block; resolve it for display.
  readonly secondOctet = computed(() =>
    resolveSecondOctet(this.firstOctet(), this.rawSecondOctet()),
  )

  private readonly rawSecondOctet = toSignal(
    tuiControlValue<number>(this.ip.controls.secondOctet),
    { requireSync: true },
  )

  readonly range = computed(() => getSecondOctetRange(this.firstOctet()))

  // 192.168.0.0/16 is a single /16 — lock the field rather than offer a
  // one-value "range".
  readonly secondOctetFixed = computed(() => {
    const { min, max } = this.range()
    return min === max
  })

  // Block-range validator for the selected first octet, bound declaratively via
  // [tuiValidator]. Changing the block produces a new validator, which the
  // directive re-registers — so the control re-validates (driving form.invalid /
  // save-blocking) without any imperative setValidators/updateValueAndValidity.
  readonly validator = computed(() => {
    const { min, max } = this.range()
    return secondOctetBlockValidator(min, max)
  })

  // Inline allowed-range hint, shown immediately (even untouched) so a block
  // switch that invalidates the carried-over octet is visible without re-touching
  // the field. Form validity itself is enforced by [tuiValidator]; this computed
  // is display only.
  readonly secondOctetError = computed(() => {
    if (this.secondOctetFixed()) return null
    const second = this.rawSecondOctet()
    if (second == null) return 'Required'
    const { min, max } = this.range()
    return isSecondOctetInRange(this.firstOctet(), second)
      ? null
      : `Allowed: ${min}–${max}`
  })
}
