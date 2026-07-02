import { Component, effect, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import { TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import {
  injectFormService,
  provideFormService,
} from 'src/app/services/form.service'
import { WanIpv4Ip } from './form/ip'
import { WanIpv4Service } from './service'
import { WanIpv4Summary } from './summary'
import { getWanIpv4Form, updateIpv4Validators, WanIpv4Form } from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article wanIpv4Summary [formLoading]="!service.data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="form.reset(service.data())"
      (ngSubmit)="onSave()"
    >
      <wan-ipv4-ip formGroupName="ip" />
      @if (service.data()) {
        <footer appFooter></footer>
      }
    </form>
  `,
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    Footer,
    Form,
    WanIpv4Summary,
    WanIpv4Ip,
    i18nPipe,
  ],
  host: { class: 'g-page' },
  providers: [provideFormService(WanIpv4Service)],
})
export default class WanIpv4 {
  protected readonly builder = inject(NonNullableFormBuilder)
  protected readonly service = injectFormService<WanIpv4Form>()

  readonly form = getWanIpv4Form(this.builder)

  readonly ipMode = toSignal(
    this.form.controls.ip.controls.mode.valueChanges.pipe(
      startWith(this.form.controls.ip.controls.mode.value),
    ),
    { requireSync: true },
  )

  constructor() {
    // Reset form when data loads
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
        updateIpv4Validators(this.form, data.ip.mode)
      }
    })

    // Update validators when IP mode changes
    effect(() => {
      const mode = this.ipMode()
      if (mode) {
        updateIpv4Validators(this.form, mode)
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
