import { Component, effect, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { TuiAnimated, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiError,
  TuiInput,
  TuiLabel,
  TuiRadio,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiElasticContainer, TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import {
  injectFormService,
  provideFormService,
} from 'src/app/services/form.service'
import { CustomValidators } from 'src/app/utils/validators'
import { DnsService } from './service'
import {
  DNS_LABELS,
  DNS_MODES,
  DNS_VALIDATION_ERRORS,
  DnsForm,
  getDnsForm,
  updateDnsValidators,
} from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="form.reset(service.data())"
      (ngSubmit)="onSave()"
    >
      <section>
        @for (mode of dnsModes; track $index) {
          <label tuiLabel>
            <input
              type="radio"
              tuiRadio
              formControlName="mode"
              [value]="mode"
            />
            {{ labels[mode] | i18n }}{{ $first ? (' (Default)' | i18n) : '' }}
          </label>
        }
      </section>
      <tui-elastic-container>
        @if (mode() === 'custom') {
          @for (label of ['Primary', 'Secondary', 'Tertiary']; track $index) {
            <section tuiAnimated>
              <tui-textfield>
                <label tuiLabel>
                  {{ label | i18n }}{{ $first ? '' : (' (optional)' | i18n) }}
                </label>
                <input tuiInput [formControlName]="'custom' + ($index + 1)" />
              </tui-textfield>
              <label tuiLabel>
                <input
                  type="checkbox"
                  tuiSwitch
                  [formControlName]="'custom' + ($index + 1) + 'Tls'"
                />
                {{ 'Secure (DoH)' | i18n }}
              </label>
            </section>
            <tui-error [formControlName]="'custom' + ($index + 1)" />
          }
        }
      </tui-elastic-container>
      @if (service.data()) {
        <footer appFooter></footer>
      }
    </form>
  `,
  host: { class: 'g-page' },
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiTextfield,
    TuiError,
    TuiInput,
    TuiLabel,
    TuiRadio,
    TuiSwitch,
    Form,
    Footer,
    TuiElasticContainer,
    TuiAnimated,
    i18nPipe,
  ],
  providers: [
    provideFormService(DnsService),
    provideTranslatedValidationErrors(DNS_VALIDATION_ERRORS),
  ],
})
export default class Dns {
  protected readonly builder = inject(NonNullableFormBuilder)
  protected readonly service = injectFormService<DnsForm>()
  protected readonly labels = DNS_LABELS
  protected readonly dnsModes = DNS_MODES
  protected readonly form = getDnsForm(this.builder, [CustomValidators.ipv4()])
  protected readonly mode = toSignal(
    this.form.controls.mode.valueChanges.pipe(
      startWith(this.form.controls.mode.value),
    ),
    { requireSync: true },
  )

  constructor() {
    // Reset form when data loads
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
        updateDnsValidators(this.form, data.mode, [CustomValidators.ipv4()])
      }
    })

    // Update validators when mode changes
    effect(() => {
      const mode = this.mode()
      if (mode) {
        updateDnsValidators(this.form, mode, [CustomValidators.ipv4()])
      }
    })
  }

  async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
    } else if (await this.service.save(this.form.getRawValue())) {
      this.form.markAsPristine()
    }
  }
}
