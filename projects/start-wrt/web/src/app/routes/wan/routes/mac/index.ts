import { Component, effect, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiError,
  TuiInput,
  TuiLabel,
  TuiRadio,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiElasticContainer, TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import {
  injectFormService,
  provideFormService,
} from 'src/app/services/form.service'
import { MacService } from './service'
import { MacSummary } from './summary'
import {
  getMacForm,
  MAC_LABELS,
  MAC_STRATEGIES,
  MAC_VALIDATION_ERRORS,
  MacForm,
  updateMacValidators,
} from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article macSummary [formLoading]="!service.data()"></article>
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
        @for (strategy of strategies; track strategy) {
          <label tuiLabel>
            <input
              type="radio"
              tuiRadio
              formControlName="strategy"
              [value]="strategy"
            />
            {{ labels[strategy] | i18n
            }}{{ $first ? (' (Default)' | i18n) : '' }}
          </label>
        }
      </section>
      <tui-elastic-container formGroupName="address">
        @if (strategy() === 'custom') {
          <tui-textfield [style.max-inline-size.rem]="13">
            <label tuiLabel>{{ labels.mac | i18n }}</label>
            <input tuiInput formControlName="mac" />
          </tui-textfield>
          <tui-error formControlName="mac" />
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
    TuiRadio,
    TuiLabel,
    Form,
    Footer,
    MacSummary,
    TuiInput,
    TuiError,
    TuiElasticContainer,
    i18nPipe,
  ],
  providers: [
    provideFormService(MacService),
    provideTranslatedValidationErrors(MAC_VALIDATION_ERRORS),
  ],
})
export default class Mac {
  protected readonly builder = inject(NonNullableFormBuilder)
  protected readonly service = injectFormService<MacForm>()
  protected readonly form = getMacForm(this.builder)
  protected readonly strategies = MAC_STRATEGIES
  protected readonly labels = MAC_LABELS

  readonly strategy = toSignal(
    this.form.controls.strategy.valueChanges.pipe(
      startWith(this.form.controls.strategy.value),
    ),
    { requireSync: true },
  )

  constructor() {
    // Reset form when data loads
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
        updateMacValidators(this.form, data.strategy)
      }
    })

    // Update validators when strategy changes
    effect(() => {
      const strategy = this.strategy()
      if (strategy) {
        updateMacValidators(this.form, strategy)
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
