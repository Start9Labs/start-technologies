import { Component, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { MaskitoDirective } from '@maskito/angular'
import { TuiAnimated, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiError,
  TuiHint,
  TuiInput,
  TuiLabel,
  tuiNumberFormatProvider,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiElasticContainer, TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { ApiService } from 'src/app/services/api/api.service'
import {
  injectFormService,
  provideFormService,
} from 'src/app/services/form.service'
import { PREFIX } from 'src/app/utils/masks'

import { LanIpv6Service } from './service'
import { LanIpv6Summary } from './summary'
import { LanIpv6Data } from './service'
import {
  getLanIpv6Form,
  PREFIX_VALIDATION_ERRORS,
  updateLanIpv6Validators,
} from './utils'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article lanIpv6Summary [formLoading]="!service.data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="form.reset(service.data())"
      (ngSubmit)="onSave()"
    >
      <ng-container formGroupName="strategy">
        <label tuiLabel>
          <input type="checkbox" tuiSwitch formControlName="slaac" />
          {{ 'Enable' | i18n }}
          <i
            [tuiHint]="
              slaacLocked()
                ? ('Cannot disable: devices with static IPv6 reservations exist'
                  | i18n)
                : ''
            "
          ></i>
        </label>
      </ng-container>
      <tui-elastic-container formGroupName="subnet">
        @if (slaacEnabled()) {
          <tui-textfield tuiAnimated>
            <label tuiLabel>{{ 'Prefix Length' | i18n }}</label>
            <input tuiInput formControlName="prefix" [maskito]="mask" />
          </tui-textfield>
          <tui-error formControlName="prefix" />
        }
      </tui-elastic-container>
      @if (service.data()) {
        <footer appFooter></footer>
      }
    </form>
  `,
  styles: `
    tui-textfield {
      max-width: 10rem;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    Footer,
    Form,
    LanIpv6Summary,
    TuiLabel,
    TuiSwitch,
    TuiHint,
    TuiError,
    TuiInput,
    TuiElasticContainer,
    MaskitoDirective,
    TuiAnimated,
    i18nPipe,
  ],
  providers: [
    provideFormService(LanIpv6Service),
    tuiNumberFormatProvider({ precision: 0 }),
    provideTranslatedValidationErrors(PREFIX_VALIDATION_ERRORS),
  ],
})
export default class LanIpv6 {
  protected readonly builder = inject(NonNullableFormBuilder)
  private readonly api = inject(ApiService)

  protected readonly service = injectFormService<LanIpv6Data>()
  protected readonly form = getLanIpv6Form(this.builder)
  protected readonly mask = PREFIX
  readonly slaacLocked = signal(false)
  protected readonly slaacEnabled = toSignal(
    this.form.controls.strategy.controls.slaac.valueChanges.pipe(
      startWith(this.form.controls.strategy.controls.slaac.value),
    ),
    { requireSync: true },
  )

  constructor() {
    // Load IPv6 dependencies
    this.loadIpv6Dependencies()

    // Reset form when data loads
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
        updateLanIpv6Validators(
          this.form,
          data.strategy.slaac,
          data.wanPrefix ?? 48,
        )
      }
    })

    // Update validators when SLAAC mode changes
    effect(() => {
      updateLanIpv6Validators(
        this.form,
        this.slaacEnabled(),
        this.service.data()?.wanPrefix ?? 48,
      )
    })
  }

  private async loadIpv6Dependencies() {
    const devices = await this.api.devicesList()
    if (devices.some(d => d.ipv6_static)) {
      this.slaacLocked.set(true)
      if (this.slaacEnabled()) {
        this.form.controls.strategy.controls.slaac.disable()
      }
    }
  }

  async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
    } else if (await this.service.save(this.form.getRawValue())) {
      this.form.markAsPristine()
    }
  }
}
