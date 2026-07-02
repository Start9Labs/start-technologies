import { Component, computed, effect, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { TuiAnimated, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import { TuiTextfield, TuiTitle } from '@taiga-ui/core'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiSelect,
  TuiSwitch,
} from '@taiga-ui/kit'
import { TuiElasticContainer, TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import {
  injectFormService,
  provideFormService,
} from 'src/app/services/form.service'
import { DdnsFields } from './form/fields'
import { DdnsService } from './service'
import { DdnsSummary } from './summary'
import {
  DDNS_PROVIDER_LIST,
  DDNS_PROVIDERS,
  DdnsForm,
  DdnsProvider,
  getDdnsForm,
  getProviderFields,
  updateDdnsValidators,
} from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article ddnsSummary [formLoading]="!service.data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="form.reset(service.data())"
      (ngSubmit)="onSave()"
    >
      <label tuiLabel>
        <input type="checkbox" tuiSwitch formControlName="enabled" />
        {{ 'Enable Dynamic DNS' | i18n }}
      </label>
      <tui-elastic-container>
        @if (enabled()) {
          <section tuiAnimated>
            <tui-textfield tuiChevron>
              <label tuiLabel>{{ 'Provider' | i18n }}</label>
              <input tuiSelect formControlName="provider" />
              <tui-data-list-wrapper
                *tuiDropdown
                [itemContent]="providerContent"
                [items]="providerList"
              />
              <ng-template #providerContent let-item>
                {{ getProviderLabel(item) }}
              </ng-template>
            </tui-textfield>
          </section>
        }
      </tui-elastic-container>
      <tui-elastic-container>
        @if (enabled() && providerFields().length) {
          <ddns-fields
            tuiAnimated
            formGroupName="fields"
            [fields]="providerFields()"
          />
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
    TuiSwitch,
    TuiChevron,
    TuiSelect,
    TuiDataListWrapper,
    Form,
    Footer,
    DdnsSummary,
    DdnsFields,
    TuiElasticContainer,
    TuiAnimated,
    i18nPipe,
  ],
  providers: [provideFormService(DdnsService)],
})
export default class Ddns {
  protected readonly builder = inject(NonNullableFormBuilder)
  protected readonly service = injectFormService<DdnsForm>()

  readonly form = getDdnsForm(this.builder)

  protected readonly providerList = DDNS_PROVIDER_LIST

  getProviderLabel(provider: string): string {
    return DDNS_PROVIDERS[provider as DdnsProvider]?.label ?? provider
  }

  readonly enabled = toSignal(
    this.form.controls.enabled.valueChanges.pipe(
      startWith(this.form.controls.enabled.value),
    ),
    { requireSync: true },
  )

  readonly provider = toSignal(
    this.form.controls.provider.valueChanges.pipe(
      startWith(this.form.controls.provider.value),
    ),
    { requireSync: true },
  )

  readonly providerFields = computed(() => getProviderFields(this.provider()))

  constructor() {
    // Reset form when data loads
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
        updateDdnsValidators(this.form, data.enabled, data.provider)
      }
    })

    // Update validators when enabled or provider changes
    effect(() => {
      updateDdnsValidators(this.form, this.enabled(), this.provider())
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
