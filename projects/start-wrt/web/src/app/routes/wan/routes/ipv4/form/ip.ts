import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ReactiveFormsModule } from '@angular/forms'
import { MaskitoDirective } from '@maskito/angular'
import { MaskitoOptions } from '@maskito/core'
import { TuiAnimated, tuiControlValue } from '@taiga-ui/cdk'
import {
  TuiError,
  TuiInput,
  TuiRadio,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import {
  TuiCardLarge,
  TuiElasticContainer,
  TuiForm,
  TuiHeader,
} from '@taiga-ui/layout'
import { map } from 'rxjs'
import { FORM } from 'src/app/components/form'
import WanIpv4 from '../'
import {
  IPV4_LABELS,
  IPV4_MODES,
  IPV4_PPPOE_CONTROLS,
  IPV4_STATIC_CONTROLS,
  IPV4_VALIDATION_ERRORS,
  netmaskFromPrefix,
} from '../utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: 'wan-ipv4-ip',
  template: `
    <header tuiHeader="body-l">
      <h2 tuiTitle>{{ 'IP Address' | i18n }}</h2>
    </header>
    <section>
      @for (mode of modes; track $index) {
        <label tuiLabel>
          <input type="radio" tuiRadio formControlName="mode" [value]="mode" />
          {{ labels[mode] | i18n }}{{ $index ? '' : (' (Default)' | i18n) }}
        </label>
      }
    </section>
    <tui-elastic-container>
      @if (parent.ipMode() === 'static') {
        <section tuiAnimated>
          @for (control of staticControls; track control) {
            <div>
              <tui-textfield>
                <label tuiLabel>{{ labels[control] | i18n }}</label>
                <input
                  tuiInput
                  [formControlName]="control"
                  [maskito]="control === 'prefix' ? prefixMask : null"
                />
              </tui-textfield>
              <tui-error [formControlName]="control" />
              @if (control === 'prefix' && netmask()) {
                <tui-error
                  class="g-secondary"
                  [error]="('Subnet: ' | i18n) + netmask()"
                />
              }
            </div>
          }
        </section>
      }
    </tui-elastic-container>
    <tui-elastic-container>
      @if (parent.ipMode() === 'pppoe') {
        <section tuiAnimated>
          @for (control of pppoeControls; track control) {
            <div>
              <tui-textfield>
                <label tuiLabel>
                  {{ labels[control] | i18n }}
                  {{ control === 'device' ? (' (optional)' | i18n) : '' }}
                </label>
                <input
                  tuiInput
                  [formControlName]="control"
                  [type]="control === 'password' ? 'password' : 'text'"
                />
              </tui-textfield>
              <tui-error [formControlName]="control" />
            </div>
          }
        </section>
      }
    </tui-elastic-container>
  `,
  viewProviders: [FORM],
  hostDirectives: [TuiForm, TuiCardLarge],
  providers: [provideTranslatedValidationErrors(IPV4_VALIDATION_ERRORS)],
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiTextfield,
    TuiRadio,
    TuiError,
    TuiInput,
    MaskitoDirective,
    TuiAnimated,
    TuiElasticContainer,
    i18nPipe,
  ],
})
export class WanIpv4Ip {
  protected readonly parent = inject(WanIpv4)
  protected readonly modes = IPV4_MODES
  protected readonly staticControls = IPV4_STATIC_CONTROLS
  protected readonly pppoeControls = IPV4_PPPOE_CONTROLS
  protected readonly labels = IPV4_LABELS
  protected readonly prefixMask: MaskitoOptions = { mask: ['/', /\d/, /\d/] }
  protected readonly netmask = toSignal(
    tuiControlValue<string>(this.parent.form.controls.ip.controls.prefix).pipe(
      map(value => netmaskFromPrefix(value)),
    ),
  )
}
