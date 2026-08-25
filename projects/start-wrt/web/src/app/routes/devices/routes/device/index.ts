import { Component, computed, effect, inject } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
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
import { TUI_CONFIRM, TuiSkeleton, TuiSwitch } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { catchError, EMPTY, filter, from, startWith, switchMap } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { DevicesService } from 'src/app/routes/devices/service'
import {
  DEVICE_VALIDATION_ERRORS,
  getDeviceForm,
  updateDeviceValidators,
} from 'src/app/routes/devices/utils'
import {
  ApiService,
  InjectedDnsRecordFromApi,
} from 'src/app/services/api/api.service'
import { DeviceSummary } from './summary'
import { InjectedRecordsTable } from './records'
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
              [placeholder]="data()?.name ?? ''"
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
            <input tuiInput [readOnly]="true" [value]="data()?.ipv6 ?? ''" />
          </tui-textfield>
        </div>
        <div class="g-secondary">
          {{
            'Chosen by the device — IPv6 addresses cannot be reserved' | i18n
          }}
        </div>
      </section>
      <fieldset>
        <legend>{{ 'Permissions' | i18n }}</legend>
        <section>
          <label tuiLabel>
            <input
              tuiSwitch
              type="checkbox"
              formControlName="allowAutoPortForward"
            />
            {{ 'Allow automatic port forwarding' | i18n }}
          </label>
          <div class="g-secondary">
            {{
              'Lets this device open and renew its own port forwards via UPnP/PCP (used by StartOS servers, game consoles, and similar). Off by default; active forwards appear on the Published Ports page.'
                | i18n
            }}
          </div>
        </section>
        <section>
          <label tuiLabel>
            <input
              tuiSwitch
              type="checkbox"
              formControlName="allowDnsInjection"
              (click)="onDnsInjectionToggle($event)"
            />
            {{ 'Allow DNS record publishing' | i18n }}
          </label>
          <div class="g-secondary">
            {{
              'Lets this device publish DNS names for itself into the router, so every device on your network can resolve them (used by StartOS servers for private domains). Off by default; published names appear below.'
                | i18n
            }}
          </div>
        </section>
      </fieldset>
      @if (data()) {
        <footer appFooter></footer>
      }
    </form>
    @if (deviceRecords().length) {
      <header tuiHeader="h6">
        <hgroup tuiTitle>
          <h3>{{ 'Published DNS records' | i18n }}</h3>
          <p tuiSubtitle>
            {{
              'Names this device has published into the router. They expire on their own when the device stops publishing them; turning the permission off removes them immediately.'
                | i18n
            }}
          </p>
        </hgroup>
      </header>
      <table
        [style.margin-block.rem]="1"
        [injectedRecords]="deviceRecords()"
      ></table>
    }
  `,
  styles: `
    header[tuiHeader='h6'] {
      align-items: center;
    }

    header[tuiHeader='h6'],
    table {
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
    InjectedRecordsTable,
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

  // Re-read on every device poll so the table follows records as they arrive
  // and expire, and empties right after the permission is turned off. A failed
  // read keeps the last list; the device poll already reports unreachability.
  private readonly allRecords = toSignal(
    toObservable(this.service.data).pipe(
      switchMap(() =>
        from(this.api.dnsInjectedList()).pipe(catchError(() => EMPTY)),
      ),
    ),
    { initialValue: [] as InjectedDnsRecordFromApi[] },
  )
  readonly deviceRecords = computed(() =>
    this.allRecords().filter(
      r => r.owner_mac?.toUpperCase() === this.mac.toUpperCase(),
    ),
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
          name: data.customName ?? '',
          allowAutoPortForward: data.allowAutoPortForward,
          allowDnsInjection: data.allowDnsInjection,
          ip: {
            ipv4Static: data.ipv4Static,
            ipv4: data.ipv4 ?? '',
          },
        })
        updateDeviceValidators(this.form, data.ipv4Static)
      }
    })

    // Update validators when the static toggle changes
    effect(() => {
      updateDeviceValidators(this.form, this.ipv4Static())
    })
  }

  // Publishing DNS names is a trust grant with network-wide effect, so
  // enabling asks first; the control only flips on confirmation. Disabling
  // needs no ceremony.
  protected onDnsInjectionToggle(event: Event) {
    const control = this.form.controls.allowDnsInjection
    if (control.value) return
    event.preventDefault()
    this.dialogs
      .open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Allow DNS Record Publishing?'),
        data: {
          content: this.i18n.transform(
            'This device will be able to publish DNS names that resolve on your whole network. Grant this only to a device you trust, such as your own StartOS server.',
          ),
          yes: this.i18n.transform('Allow'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        control.setValue(true)
        // The pristine-gated reset effect must not undo the choice before
        // Save.
        control.markAsDirty()
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

    // Only send a permission when it actually changed — each is a separate
    // endpoint, and revoking one tears down what the device created with it.
    const allowAutoForward =
      formValue.allowAutoPortForward !== this.data()?.allowAutoPortForward
        ? formValue.allowAutoPortForward
        : undefined
    const allowDnsInjection =
      formValue.allowDnsInjection !== this.data()?.allowDnsInjection
        ? formValue.allowDnsInjection
        : undefined

    const success = await this.service.update(
      this.mac,
      {
        name: formValue.name,
        ipv4Static: formValue.ip.ipv4Static,
        ipv4: formValue.ip.ipv4,
      },
      allowAutoForward,
      allowDnsInjection,
    )

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
        allowAutoPortForward: data.allowAutoPortForward,
        allowDnsInjection: data.allowDnsInjection,
        ip: {
          ipv4Static: data.ipv4Static,
          ipv4: data.ipv4 ?? '',
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
