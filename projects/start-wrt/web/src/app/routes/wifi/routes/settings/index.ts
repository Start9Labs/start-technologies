import { Component, computed, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  tuiMarkControlAsTouchedAndValidate,
  TuiAnimated,
  TuiValueChanges,
} from '@taiga-ui/cdk'
import {
  TuiDataList,
  TuiFilterByInputPipe,
  TuiInput,
  TuiLabel,
  TuiNotification,
  TuiRadio,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import {
  TUI_CONFIRM,
  TuiChevron,
  TuiComboBox,
  TuiDataListWrapper,
  TuiSelect,
  TuiSwitch,
} from '@taiga-ui/kit'
import { TuiElasticContainer } from '@taiga-ui/layout'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter, map, startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import {
  ApiService,
  WifiConfig,
  WifiRegulatory,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nService } from 'src/app/i18n/i18n.service'
import { WifiService } from '../../service'
import { ReconnectDialog } from './reconnect-dialog'

// The kernel's world regulatory domain; null on the wire.
const WORLD = '00'

@Component({
  template: `
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="onCancel()"
      (ngSubmit)="onSave()"
    >
      <tui-textfield tuiChevron [stringify]="stringifyCountry">
        <label tuiLabel>{{ 'Country' | i18n }}</label>
        <input
          tuiComboBox
          formControlName="country"
          (tuiValueChanges)="onCountry($event)"
        />
        <tui-data-list-wrapper
          *tuiDropdown
          [items]="countries() | tuiFilterByInput"
        />
      </tui-textfield>
      <tui-elastic-container>
        @if (unset()) {
          <div tuiAnimated tuiNotification appearance="warning">
            {{
              'Select the country this router operates in to unlock the Wi-Fi channels and transmit power permitted there. Until then it uses a conservative worldwide subset.'
                | i18n
            }}
          </div>
        }
      </tui-elastic-container>
      <label tuiLabel>
        <input type="checkbox" tuiSwitch formControlName="enabled" />
        {{ 'Enable Wi-Fi' | i18n }}
      </label>
      <tui-textfield>
        <label tuiLabel>SSID</label>
        <input tuiInput formControlName="ssid" />
      </tui-textfield>
      <label tuiLabel>
        <input type="checkbox" tuiSwitch formControlName="broadcast" />
        {{ 'Broadcast' | i18n }}
      </label>
      <fieldset>
        <legend>{{ 'Frequency Band' | i18n }}</legend>
        @for (value of bands; track $index) {
          <label tuiLabel>
            <input
              type="radio"
              tuiRadio
              formControlName="band"
              [value]="value"
            />
            {{ value | i18n }}
          </label>
        }
      </fieldset>
      <tui-elastic-container>
        @if (band() === 'Both') {
          <label tuiLabel tuiAnimated>
            <input
              type="checkbox"
              tuiSwitch
              formControlName="broadcastSeparately"
            />
            {{ 'Broadcast Separately' | i18n }}
          </label>
        }
      </tui-elastic-container>
      <fieldset>
        <legend>{{ 'Frequency Range' | i18n }}</legend>
        <tui-textfield tuiChevron [stringify]="stringifyChannel">
          <label tuiLabel>{{ '2.4 GHz Channel' | i18n }}</label>
          <input tuiSelect formControlName="channel24" />
          <tui-data-list *tuiDropdown>
            @for (ch of channels24(); track ch) {
              <button tuiOption [value]="ch">{{ ch | i18n }}</button>
            }
          </tui-data-list>
        </tui-textfield>
        <tui-textfield tuiChevron [stringify]="stringifyChannel">
          <label tuiLabel>{{ '5 GHz Channel' | i18n }}</label>
          <input tuiSelect formControlName="channel5" />
          <tui-data-list *tuiDropdown>
            @for (ch of channels5(); track ch) {
              <button tuiOption [value]="ch">{{ ch | i18n }}</button>
            }
          </tui-data-list>
        </tui-textfield>
      </fieldset>
      @if (service.data()) {
        <footer appFooter></footer>
      }
    </form>
  `,
  styles: `
    fieldset {
      display: flex !important;
    }

    tui-textfield {
      flex: 1;
      max-width: 16rem;
    }
  `,
  providers: [tuiTextfieldOptionsProvider({ cleaner: signal(false) })],
  host: { class: 'g-page' },
  imports: [
    ReactiveFormsModule,
    TuiLabel,
    TuiSwitch,
    TuiRadio,
    TuiTextfield,
    TuiInput,
    TuiSelect,
    TuiComboBox,
    TuiChevron,
    TuiDataList,
    TuiDataListWrapper,
    TuiFilterByInputPipe,
    TuiNotification,
    Footer,
    Form,
    TuiElasticContainer,
    TuiAnimated,
    TuiValueChanges,
    i18nPipe,
  ],
})
export default class WifiSettings {
  protected readonly service = inject(WifiService)
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly regionNames = new Intl.DisplayNames(
    [inject(i18nService).lang.replace('_', '-')],
    { type: 'region', fallback: 'code' },
  )
  protected readonly regulatory = signal<WifiRegulatory | null>(null)
  protected readonly form = inject(NonNullableFormBuilder).group({
    country: [WORLD],
    enabled: [true],
    ssid: ['StartOS'],
    broadcast: [true],
    band: ['Both'],
    broadcastSeparately: [false],
    channel24: ['Auto'],
    channel5: ['Auto'],
  })

  protected readonly band = toSignal(
    this.form.controls.band.valueChanges.pipe(
      startWith(this.form.controls.band.value),
    ),
    { requireSync: true },
  )

  protected readonly bands = ['2.4 GHz', '5 GHz', 'Both']

  // Typing over a selection empties the control until an item is picked.
  protected readonly unset = toSignal(
    this.form.controls.country.valueChanges.pipe(
      startWith(this.form.controls.country.value),
      map(country => !country || country === WORLD),
    ),
    { requireSync: true },
  )

  protected readonly stringifyCountry = (code: string): string =>
    code === WORLD
      ? this.i18n.transform('Not set')
      : `${this.regionNames.of(code)} (${code})`

  protected readonly countries = computed(() => [
    WORLD,
    ...[...(this.regulatory()?.countries ?? [])].sort((a, b) =>
      this.stringifyCountry(a).localeCompare(this.stringifyCountry(b)),
    ),
  ])

  // Translates the 'Auto' option; numeric channels pass through unchanged.
  protected readonly stringifyChannel = (c: string): string =>
    this.i18n.transform(c)

  protected readonly channels24 = computed(() => this.channelOptions('2g'))
  protected readonly channels5 = computed(() => this.channelOptions('5g'))

  constructor() {
    this.loadRegulatory()
    effect(() => {
      const config = this.service.data()
      if (config && this.form.pristine) {
        this.form.reset(this.toFormValue(config))
      }
    })
  }

  // A channel permitted under the old country may not be under the new one.
  protected onCountry(country: string | null): void {
    if ((country || WORLD) !== (this.service.data()?.country ?? WORLD)) {
      this.form.patchValue({ channel24: 'Auto', channel5: 'Auto' })
    }
  }

  protected onCancel(): void {
    const config = this.service.data()
    if (config) this.form.reset(this.toFormValue(config))
  }

  protected async onSave(): Promise<void> {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      return
    }

    const config = this.toConfig()
    if (!config) return

    const ssidChanged = config.ssid !== this.service.data()?.ssid

    if (ssidChanged) {
      this.dialogs
        .open(TUI_CONFIRM, {
          label: this.i18n.transform('Change SSID?'),
          data: {
            content: `${this.i18n.transform('Changing the SSID will disconnect all WiFi clients. You will need to reconnect to')} "${config.ssid}".`,
            yes: this.i18n.transform('Change SSID'),
            no: this.i18n.transform('Cancel'),
          },
        })
        .pipe(filter(Boolean))
        .subscribe(() => {
          // The SSID change disconnects every WiFi client; a transient toast
          // can't carry an instruction the user must act on. Hand off to the
          // persistent ReconnectDialog, which tells the user to rejoin the new
          // network and reloads once they have.
          const done = this.service.saveForSsidChange(config)
          this.dialogs
            .open(new PolymorpheusComponent(ReconnectDialog), {
              closable: false,
              dismissible: false,
              data: { ssid: config.ssid, done },
            })
            .subscribe({
              complete: () => {
                this.service.refresh()
                this.loadRegulatory()
                this.form.markAsPristine()
              },
            })
        })
      return
    }

    if (await this.service.saveWithRestart(config)) {
      this.loadRegulatory()
      this.form.markAsPristine()
    }
  }

  private loadRegulatory(): void {
    this.api.wifiRegulatory().then(r => this.regulatory.set(r))
  }

  private channelOptions(band: string): string[] {
    return ['Auto', ...(this.regulatory()?.channels[band] ?? []).map(String)]
  }

  private toFormValue(config: WifiConfig) {
    const radios = Object.entries(config.radios)
    const radio2g = radios.find(([, r]) => r.band === '2g' && r.enabled)
    const radio5g = radios.find(([, r]) => r.band === '5g' && r.enabled)
    const anyEnabled = radios.some(([, r]) => r.enabled)
    const anyBroadcast = radios.some(([, r]) => r.broadcast)
    const channelToOption = (ch: string) => (ch === 'auto' ? 'Auto' : ch)

    return {
      country: config.country ?? WORLD,
      enabled: anyEnabled,
      ssid: config.ssid,
      broadcast: anyBroadcast,
      band: radio2g && radio5g ? 'Both' : radio5g ? '5 GHz' : '2.4 GHz',
      broadcastSeparately: config.broadcastSeparately,
      channel24: radio2g ? channelToOption(radio2g[1].channel) : 'Auto',
      channel5: radio5g ? channelToOption(radio5g[1].channel) : 'Auto',
    }
  }

  private toConfig(): WifiConfig | null {
    const data = this.service.data()
    if (!data) return null

    const form = this.form.getRawValue()
    const optionToChannel = (ch: string) => (ch === 'Auto' ? 'auto' : ch)

    const radios: WifiConfig['radios'] = {}
    for (const [key, radio] of Object.entries(data.radios)) {
      const is2g = radio.band === '2g'
      const is5g = radio.band === '5g'
      const enabledByBand =
        form.band === 'Both' ||
        (is2g && form.band === '2.4 GHz') ||
        (is5g && form.band === '5 GHz')

      radios[key] = {
        band: radio.band,
        channel: is2g
          ? optionToChannel(form.channel24)
          : is5g
            ? optionToChannel(form.channel5)
            : radio.channel,
        enabled: form.enabled && enabledByBand,
        broadcast: form.broadcast && form.enabled && enabledByBand,
      }
    }

    return {
      ssid: form.ssid,
      broadcastSeparately: form.band === 'Both' && form.broadcastSeparately,
      country: form.country && form.country !== WORLD ? form.country : null,
      radios,
      passwords: data.passwords,
    }
  }
}
