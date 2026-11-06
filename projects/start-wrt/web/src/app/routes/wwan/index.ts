import { Component, computed, effect, inject, signal } from '@angular/core'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import type { TuiContext } from '@taiga-ui/cdk'
import { TuiButton, TuiLink, TuiNotification, TuiTitle } from '@taiga-ui/core'
import {
  TUI_CONFIRM,
  TuiBadge,
  TuiRadioList,
  TuiSegmented,
  TuiStatus,
} from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { Summary } from 'src/app/components/summary'
import {
  ApiService,
  WwanConnectRequest,
  WwanNetwork,
  WwanPriority,
  WwanResponse,
} from 'src/app/services/api/api.service'
import { ActionService } from 'src/app/services/action.service'
import { provideFormService } from 'src/app/services/form.service'
import { i18nKey } from 'src/app/i18n/i18n.providers'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { WWAN_JOIN } from './dialog'
import { WwanService } from './service'
import { WwanNetworksTable } from './table'
import { BAND_LABELS, PRIORITY_LABELS } from './utils'

interface UplinkStatus {
  label: i18nKey
  appearance: string
}

function uplinkStatus({ state }: WwanResponse): UplinkStatus {
  if (!state?.associated) {
    return { label: 'Not connected', appearance: 'negative' }
  }
  if (!state.address) {
    return { label: 'Connecting', appearance: 'warning' }
  }
  return state.active
    ? { label: 'In use', appearance: 'positive' }
    : { label: 'Standing by', appearance: 'neutral' }
}

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Wireless WAN' | i18n }}</h2>
        <p tuiSubtitle>
          {{
            'Connect the router to another Wi-Fi network for Internet access, as a backup to the WAN port or in its place.'
              | i18n
          }}
        </p>
      </hgroup>
    </header>
    @if (service.data(); as data) {
      @if (data.unmanaged.length) {
        <div tuiNotification appearance="warning">
          {{
            'A Wi-Fi uplink was set up in LuCI. Manage it there, or delete it there to manage the uplink here.'
              | i18n
          }}
          <a tuiLink [href]="luciUrl" target="_blank">
            {{ 'Launch LuCI Interface' | i18n }}
          </a>
        </div>
      } @else if (data.uplink; as uplink) {
        <header tuiHeader="h6">
          <h3 tuiTitle>{{ 'Summary' | i18n }}</h3>
          <aside tuiAccessories>
            <button
              tuiButton
              size="s"
              appearance="secondary-destructive"
              (click)="disconnect()"
            >
              {{ 'Remove uplink' | i18n }}
            </button>
          </aside>
        </header>
        <article [formLoading]="false">
          <section>
            <div [appSummary]="uplink.ssid">{{ 'Network name' | i18n }}</div>
            @if (status(); as s) {
              <div appSummary>
                {{ 'Status' | i18n }}
                <span tuiSubtitle>
                  <span tuiBadge tuiStatus [appearance]="s.appearance">
                    {{ s.label | i18n }}
                  </span>
                </span>
              </div>
            }
            <div [appSummary]="data.state?.address ?? ''">
              {{ 'Assigned IP' | i18n }}
            </div>
            <div appSummary>
              {{ 'Signal' | i18n }}
              @if (data.state?.signal; as signal) {
                <span tuiSubtitle>{{ signal }} dBm</span>
              }
            </div>
            <div appSummary>
              {{ 'Band' | i18n }}
              <span tuiSubtitle>{{ band(uplink.band) | i18n }}</span>
            </div>
          </section>
        </article>
        <header tuiHeader="h6">
          <h3 tuiTitle>{{ 'Settings' | i18n }}</h3>
        </header>
        <form
          [formGroup]="form"
          [formLoading]="false"
          (reset.prevent)="form.reset({ priority: uplink.priority })"
          (ngSubmit)="savePriority()"
        >
          <fieldset>
            <legend>{{ 'Priority' | i18n }}</legend>
            <tui-radio-list
              formControlName="priority"
              [items]="priorities"
              [itemContent]="priority"
            />
          </fieldset>
          <div tuiNotification appearance="info">
            {{
              'While the router is not connected to this network, its own Wi-Fi on this band is off.'
                | i18n
            }}
          </div>
          <footer appFooter></footer>
        </form>
      } @else if (radios().length) {
        <header tuiHeader="h6">
          <h3 tuiTitle>{{ 'Available networks' | i18n }}</h3>
          <aside tuiAccessories>
            @if (radios().length > 1) {
              <tui-segmented
                size="s"
                [(activeItemIndex)]="radioIndex"
                (activeItemIndexChange)="networks.set(null)"
              >
                @for (r of radios(); track r.radio) {
                  <button type="button">{{ band(r.band) | i18n }}</button>
                }
              </tui-segmented>
            }
            <button
              tuiButton
              size="s"
              appearance="secondary"
              type="button"
              (click)="join()"
            >
              {{ 'Hidden network' | i18n }}
            </button>
            <button
              tuiButton
              size="s"
              iconStart="@tui.refresh-cw"
              type="button"
              (click)="scan()"
            >
              {{ 'Scan' | i18n }}
            </button>
          </aside>
        </header>
        @if (networks(); as list) {
          <table [wwanNetworks]="list" (join)="join($event)"></table>
        } @else {
          <p class="g-secondary">
            {{ 'Scan to list the Wi-Fi networks this band can reach.' | i18n }}
          </p>
        }
      } @else {
        <div tuiNotification appearance="info">
          {{ 'Turn on Wi-Fi to join a network.' | i18n }}
          <a tuiLink routerLink="/wifi">{{ 'Wi-Fi' | i18n }}</a>
        </div>
      }
    } @else {
      <article [formLoading]="true" [style.min-height.rem]="10"></article>
    }
  `,
  host: { class: 'g-page' },
  providers: [provideFormService(WwanService)],
  imports: [
    ReactiveFormsModule,
    RouterLink,
    TuiHeader,
    TuiTitle,
    TuiButton,
    TuiLink,
    TuiNotification,
    TuiBadge,
    TuiStatus,
    TuiRadioList,
    TuiSegmented,
    Footer,
    Form,
    Summary,
    WwanNetworksTable,
    i18nPipe,
  ],
})
export default class Wwan {
  protected readonly service = inject(WwanService)
  private readonly api = inject(ApiService)
  private readonly actions = inject(ActionService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  protected readonly luciUrl = '/cgi-bin/luci'
  protected readonly priorities: WwanPriority[] = ['backup', 'preferred']
  protected readonly form = inject(NonNullableFormBuilder).group({
    priority: ['backup' as WwanPriority],
  })

  protected readonly radios = computed(
    () => this.service.data()?.radios.filter(r => r.enabled) ?? [],
  )
  protected readonly radioIndex = signal(0)
  protected readonly networks = signal<WwanNetwork[] | null>(null)
  protected readonly status = computed(() => {
    const data = this.service.data()
    return data ? uplinkStatus(data) : null
  })

  protected readonly band = (band: string): i18nKey =>
    BAND_LABELS[band] ?? 'Wi-Fi'

  protected readonly priority = ({ $implicit }: TuiContext<WwanPriority>) =>
    this.i18n.transform(PRIORITY_LABELS[$implicit])

  constructor() {
    effect(() => {
      const priority = this.service.data()?.uplink?.priority
      if (priority && this.form.pristine) this.form.reset({ priority })
    })
  }

  private selectedRadio() {
    return this.radios()[this.radioIndex()] ?? this.radios()[0]
  }

  protected async scan(): Promise<void> {
    const radio = this.selectedRadio()
    if (!radio) return
    await this.actions.run(
      async () =>
        this.networks.set(await this.api.wwanScan({ radio: radio.radio })),
      { loading: this.i18n.transform('Scanning') },
    )
  }

  protected join(network?: WwanNetwork): void {
    const radio = this.selectedRadio()
    if (!radio) return
    this.dialogs
      .open<WwanConnectRequest>(WWAN_JOIN, {
        data: { radio: radio.radio, band: radio.band, network },
      })
      .subscribe(async request => {
        if (await this.service.connect(request)) this.networks.set(null)
      })
  }

  protected async savePriority(): Promise<void> {
    if (await this.service.setPriority(this.form.getRawValue().priority)) {
      this.form.markAsPristine()
    }
  }

  protected disconnect(): void {
    this.dialogs
      .open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Remove uplink?'),
        data: {
          content: this.i18n.transform(
            'The router will leave this network. Its Internet access then depends on the WAN port alone.',
          ),
          yes: this.i18n.transform('Remove uplink'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.service.disconnect())
  }
}
