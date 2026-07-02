import { Component, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import { TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { ApiService } from 'src/app/services/api/api.service'
import {
  injectFormService,
  provideFormService,
} from 'src/app/services/form.service'
import { WanIpv6Ip } from './form/ip'
import { WanIpv6Service } from './service'
import { WanIpv6Summary } from './summary'
import { getWanIpv6Form, updateIpv6Validators, WanIpv6Form } from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article wanIpv6Summary [formLoading]="!service.data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="form.reset(service.data())"
      (ngSubmit)="onSave()"
    >
      <wan-ipv6-ip formGroupName="ip" />
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
    WanIpv6Summary,
    WanIpv6Ip,
    i18nPipe,
  ],
  host: { class: 'g-page' },
  providers: [provideFormService(WanIpv6Service)],
})
export default class WanIpv6 {
  protected readonly builder = inject(NonNullableFormBuilder)
  private readonly api = inject(ApiService)

  protected readonly service = injectFormService<WanIpv6Form>()
  protected readonly form = getWanIpv6Form(this.builder)

  // Track if any published port uses IPv6
  readonly hasIpv6Ports = signal(false)
  readonly ipMode = toSignal(
    this.form.controls.ip.controls.mode.valueChanges.pipe(
      startWith(this.form.controls.ip.controls.mode.value),
    ),
    { requireSync: true },
  )

  constructor() {
    // Load IPv6 port usage
    this.loadIpv6PortUsage()

    // Reset form when data loads
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
        updateIpv6Validators(this.form, data.ip.mode)
      }
    })

    // Update validators when IP mode changes
    effect(() => {
      const mode = this.ipMode()
      if (mode) {
        updateIpv6Validators(this.form, mode)
      }
    })
  }

  private async loadIpv6PortUsage() {
    const ports = await this.api.publishedPortsList()
    this.hasIpv6Ports.set(ports.some(p => p.ipv6 && p.enabled))
  }

  async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
    } else if (await this.service.save(this.form.getRawValue())) {
      this.form.markAsPristine()
    }
  }
}
