import { Component, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { tuiMarkControlAsTouchedAndValidate, TuiAnimated } from '@taiga-ui/cdk'
import {
  TuiDataList,
  TuiInput,
  TuiLabel,
  TuiRadio,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiChevron, TuiSelect, TuiSwitch } from '@taiga-ui/kit'
import { TuiElasticContainer } from '@taiga-ui/layout'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter, startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { WifiConfig } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { WifiService } from '../../service'
import { ReconnectDialog } from './reconnect-dialog'

@Component({
  template: `
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="onCancel()"
      (ngSubmit)="onSave()"
    >
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
        <!-- @TODO: Implement channel optimization (requires backend channel scan endpoint) -->
        <tui-textfield tuiChevron [stringify]="stringifyChannel">
          <label tuiLabel>{{ '2.4 GHz Channel' | i18n }}</label>
          <input tuiSelect formControlName="channel24" />
          <tui-data-list *tuiDropdown>
            @for (ch of channels24; track ch) {
              <button tuiOption [value]="ch">{{ ch | i18n }}</button>
            }
          </tui-data-list>
        </tui-textfield>
        <tui-textfield tuiChevron [stringify]="stringifyChannel">
          <label tuiLabel>{{ '5 GHz Channel' | i18n }}</label>
          <input tuiSelect formControlName="channel5" />
          <tui-data-list *tuiDropdown>
            @for (ch of channels5; track ch) {
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
    TuiChevron,
    TuiDataList,
    Footer,
    Form,
    TuiElasticContainer,
    TuiAnimated,
    i18nPipe,
  ],
})
export default class WifiSettings {
  protected readonly service = inject(WifiService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)
  protected readonly form = inject(NonNullableFormBuilder).group({
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

  // Translates the 'Auto' option; numeric channels pass through unchanged.
  protected readonly stringifyChannel = (c: string): string =>
    this.i18n.transform(c)

  protected readonly channels24 = [
    'Auto',
    '1',
    '2',
    '3',
    '4',
    '5',
    '6',
    '7',
    '8',
    '9',
    '10',
    '11',
  ]

  protected readonly channels5 = [
    'Auto',
    '36',
    '40',
    '44',
    '48',
    '52',
    '56',
    '60',
    '64',
    '100',
    '104',
    '108',
    '112',
    '116',
    '120',
    '124',
    '128',
    '132',
    '136',
    '140',
    '144',
    '149',
    '153',
    '157',
    '161',
    '165',
  ]

  constructor() {
    effect(() => {
      const config = this.service.data()
      if (config && this.form.pristine) {
        this.form.reset(this.toFormValue(config))
      }
    })
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
                this.form.markAsPristine()
              },
            })
        })
      return
    }

    if (await this.service.saveWithRestart(config)) {
      this.form.markAsPristine()
    }
  }

  private toFormValue(config: WifiConfig) {
    const radios = Object.entries(config.radios)
    const radio2g = radios.find(([, r]) => r.band === '2g' && r.enabled)
    const radio5g = radios.find(([, r]) => r.band === '5g' && r.enabled)
    const anyEnabled = radios.some(([, r]) => r.enabled)
    const anyBroadcast = radios.some(([, r]) => r.broadcast)
    const channelToOption = (ch: string) => (ch === 'auto' ? 'Auto' : ch)

    return {
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
      radios,
      passwords: data.passwords,
    }
  }
}
