import { Component, computed, effect, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiButton,
  TuiError,
  TuiHintDirective,
  TuiInput,
  TuiLink,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiSkeleton, TuiSwitch } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { DevicesService } from 'src/app/routes/devices/service'
import {
  DEVICE_VALIDATION_ERRORS,
  getDeviceForm,
  updateDeviceValidators,
} from 'src/app/routes/devices/utils'
import { ApiService } from 'src/app/services/api/api.service'
import { DeviceSummary } from './summary'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>
          <a
            tuiLink
            [routerLink]="returnUrl"
            appearance=""
            iconStart="@tui.chevron-left"
            [tuiSkeleton]="!data()"
            [style.font]="'inherit'"
            [style.text-decoration]="'none'"
          >
            {{ data()?.name || ('Device' | i18n) }}
          </a>
        </h2>
      </hgroup>
    </header>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
      <aside tuiAccessories>
        @if (data() && data()?.status !== 'online') {
          <button
            tuiButton
            size="m"
            appearance="secondary"
            (click)="onForget()"
          >
            {{ 'Forget' | i18n }}
          </button>
        }
      </aside>
    </header>
    <article deviceSummary [formLoading]="!data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!data()"
      (reset.prevent)="onCancel()"
      (ngSubmit)="onSave()"
    >
      <section>
        <div>
          <tui-textfield>
            <label tuiLabel>{{ 'Name' | i18n }}</label>
            <input
              tuiInput
              formControlName="name"
              [placeholder]="data()?.hostname ?? ''"
            />
          </tui-textfield>
          <tui-error formControlName="name" />
        </div>
      </section>
      <section formGroupName="ip">
        <div>
          <tui-textfield>
            <label tuiLabel>{{ 'IPv4 address' | i18n }}</label>
            <input tuiInput formControlName="ipv4" [readOnly]="!ipv4Static()" />
          </tui-textfield>
          <tui-error formControlName="ipv4" />
        </div>
        <label tuiLabel>
          <input tuiSwitch type="checkbox" formControlName="ipv4Static" />
          {{ 'Reserve' | i18n }}
          <i [tuiHint]="'Required by a published port rule' | i18n"></i>
        </label>
        <div>
          <tui-textfield>
            <label tuiLabel>{{ 'IPv6 address' | i18n }}</label>
            <input tuiInput formControlName="ipv6" [readOnly]="!ipv6Static()" />
          </tui-textfield>
          <tui-error formControlName="ipv6" />
        </div>
        <label tuiLabel>
          <input tuiSwitch type="checkbox" formControlName="ipv6Static" />
          {{ 'Reserve' | i18n }}
          <i [tuiHint]="'Required by a published port rule' | i18n"></i>
        </label>
      </section>
      @if (data()) {
        <footer appFooter></footer>
      }
    </form>
  `,
  styles: `
    header[tuiHeader='h6'] {
      align-items: center;
      max-width: 50rem;
    }
  `,
  host: { class: 'g-page' },
  providers: [provideTranslatedValidationErrors(DEVICE_VALIDATION_ERRORS)],
  imports: [
    RouterLink,
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiLink,
    TuiButton,
    Footer,
    Form,
    DeviceSummary,
    TuiSkeleton,
    TuiInput,
    TuiError,
    TuiHintDirective,
    TuiSwitch,
    i18nPipe,
  ],
})
export default class DeviceDetail {
  private readonly route = inject(ActivatedRoute)
  private readonly router = inject(Router)
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  readonly service = inject(DevicesService)
  readonly mac = this.route.snapshot.queryParams['mac']
  readonly returnUrl = history.state?.['returnUrl'] || '/devices'
  readonly form = getDeviceForm(inject(NonNullableFormBuilder))
  readonly data = computed(
    () => this.service.data()?.find(d => d.mac === this.mac) ?? null,
  )

  readonly ipv4Static = toSignal(
    this.form.controls.ip.controls.ipv4Static.valueChanges.pipe(
      startWith(this.form.controls.ip.controls.ipv4Static.value),
    ),
    { requireSync: true },
  )

  readonly ipv6Static = toSignal(
    this.form.controls.ip.controls.ipv6Static.valueChanges.pipe(
      startWith(this.form.controls.ip.controls.ipv6Static.value),
    ),
    { requireSync: true },
  )

  constructor() {
    // Refresh device data to get latest info
    this.service.refresh()

    // Load published port usage for this device
    this.loadDependencies()

    // Reset form when data loads
    effect(() => {
      const data = this.data()
      if (data && this.form.pristine) {
        this.form.reset({
          name: data.name,
          ip: {
            ipv4Static: data.ipv4Static,
            ipv4: data.ipv4 ?? '',
            ipv6Static: data.ipv6Static,
            ipv6: data.ipv6 ?? '',
          },
        })
        updateDeviceValidators(this.form, data.ipv4Static, data.ipv6Static)
      }
    })

    // Update validators when static toggles change
    effect(() => {
      updateDeviceValidators(this.form, this.ipv4Static(), this.ipv6Static())
    })
  }

  private async loadDependencies() {
    const ports = await this.api.publishedPortsList()
    const macUpper = this.mac.toUpperCase()
    const devicePorts = ports.filter(
      p => p.device_mac.toUpperCase() === macUpper && p.enabled,
    )
    if (devicePorts.some(p => p.ipv4)) {
      this.form.controls.ip.controls.ipv4Static.disable()
    }
    if (devicePorts.some(p => p.ipv6)) {
      this.form.controls.ip.controls.ipv6Static.disable()
    }
  }

  async onSave() {
    if (this.form.invalid) return

    const formValue = this.form.getRawValue()
    // A reservation only reaches the device when it next runs DHCP — the router
    // can't push it to a connected client. Warn only when we're actually pinning
    // it to a *different* address; reserving the device's existing address (just
    // making the current lease static) changes nothing for the device.
    const ipv4Changed =
      formValue.ip.ipv4Static && formValue.ip.ipv4 !== (this.data()?.ipv4 ?? '')

    const success = await this.service.update(this.mac, {
      name: formValue.name,
      ipv4Static: formValue.ip.ipv4Static,
      ipv4: formValue.ip.ipv4,
      ipv6Static: formValue.ip.ipv6Static,
      ipv6: formValue.ip.ipv6,
    })

    if (success) {
      this.form.markAsPristine()
      if (ipv4Changed) this.showIpChangedDialog()
    }
  }

  private showIpChangedDialog() {
    this.dialogs
      .open(
        this.i18n.transform(
          'The new IP address takes effect the next time this device requests one from the router — the router cannot push it to a connected device. The fastest way to apply it is to disconnect and reconnect the device (Wi-Fi or Ethernet) or reboot it; this usually works but is not guaranteed to take effect immediately. Otherwise the device will pick up the new address on its own within up to 12 hours.',
        ),
        {
          label: this.i18n.transform('IP Address Changed'),
          data: this.i18n.transform('Got it'),
        },
      )
      .subscribe()
  }

  onCancel() {
    const data = this.data()
    if (data) {
      this.form.reset({
        name: data.name,
        ip: {
          ipv4Static: data.ipv4Static,
          ipv4: data.ipv4 ?? '',
          ipv6Static: data.ipv6Static,
          ipv6: data.ipv6 ?? '',
        },
      })
    }
  }

  async onForget() {
    if (await this.service.forget(this.mac)) {
      this.router.navigate(['..'], { relativeTo: this.route })
    }
  }
}
