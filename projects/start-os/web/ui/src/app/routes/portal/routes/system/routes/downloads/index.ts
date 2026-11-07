import { Component, computed, inject, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { FormControl, ReactiveFormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCell,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { tap } from 'rxjs'

import { InterfaceComponent } from 'src/app/routes/portal/components/interfaces/interface.component'
import { InterfaceService } from 'src/app/routes/portal/components/interfaces/interface.service'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { GatewayService } from 'src/app/services/gateway.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { DOWNLOADS_MENU_ITEM } from '../../system.const'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ entry.item | i18n }}
      </div>
    </ng-container>
    <section tuiCardLarge>
      <header tuiHeader="body-l">
        <h2 tuiTitle>{{ entry.item | i18n }}</h2>
        <a
          tuiIconButton
          docsLink
          path="/start-os/marketplace.html"
          fragment="#software-sharing"
          size="xs"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
      </header>
      @let settings = preferences();
      @let configuration = config();
      @if (configuration === 'error') {
        <div tuiNotification appearance="negative">
          {{ 'Sharing settings could not be loaded.' | i18n }}
          <button tuiButton appearance="flat" size="s" (click)="load()">
            {{ 'Retry' | i18n }}
          </button>
        </div>
      } @else if (configuration === 'loading' || !settings) {
        <tui-loader />
      } @else {
        @if (!configuration.available) {
          <div tuiNotification appearance="info">
            {{ 'BitTorrent is disabled in the server configuration.' | i18n }}
          </div>
        }
        <label tuiCell>
          <span tuiTitle>
            <b>{{ 'Download using BitTorrent' | i18n }}</b>
            <span tuiSubtitle>
              {{
                'Prefer peers for software downloads, with HTTP fallback.'
                  | i18n
              }}
            </span>
          </span>
          <input tuiSwitch type="checkbox" [formControl]="controls.downloads" />
        </label>
        <label tuiCell>
          <span tuiTitle>
            <b>{{ 'Share S9PKs and OS images' | i18n }}</b>
            <span tuiSubtitle>
              {{
                'Share compatible installed S9PKs and OS images already stored on this server.'
                  | i18n
              }}
            </span>
          </span>
          <input tuiSwitch type="checkbox" [formControl]="controls.seeding" />
        </label>
        <label tuiCell>
          <span tuiTitle>
            <b>{{ 'Allow incoming peer connections' | i18n }}</b>
            <span tuiSubtitle>
              {{
                'Seeding can use outgoing connections without a forwarded port. Incoming connections let more peers connect.'
                  | i18n
              }}
            </span>
          </span>
          <input tuiSwitch type="checkbox" [formControl]="controls.incoming" />
        </label>
        <p class="g-secondary">
          {{
            'Turning off a setting also turns off the settings below it. Turning it back on does not restore them.'
              | i18n
          }}
        </p>
        <p class="g-secondary">
          {{
            'Active BitTorrent downloads can upload even when sharing stored software is off.'
              | i18n
          }}
        </p>
      }
      <div tuiNotification appearance="info">
        {{
          'HTTP downloads reveal an IP address to the server. BitTorrent widens the audience to trackers and other peers.'
            | i18n
        }}
      </div>
      <p class="g-secondary">
        {{
          'Outgoing BitTorrent connections use the StartOS default gateway. With StartTunnel, peers see its exit IP. Incoming connections expose the enabled addresses below.'
            | i18n
        }}
      </p>
      <p class="g-secondary">
        {{
          'Sharing uses upload bandwidth, not extra payload copies. Installation, uninstall and image cleanup control file retention.'
            | i18n
        }}
      </p>
    </section>
    <section tuiCardLarge>
      <header tuiHeader="body-l">
        <h3 tuiTitle>{{ 'Default gateway for StartOS' | i18n }}</h3>
        <a
          tuiButton
          routerLink="../outbound-traffic"
          size="s"
          appearance="flat"
        >
          {{ 'Change' | i18n }}
        </a>
      </header>
      @if (gateways.defaultOutboundGateway(); as gateway) {
        <span tuiTitle>
          <b>{{ gateway.name }} ({{ gateway.id }})</b>
          <span tuiSubtitle>{{ gateway.wanIp || ('Unknown' | i18n) }}</span>
        </span>
      } @else {
        <span>{{ 'Auto' | i18n }}</span>
        @for (
          gateway of gateways.systemDefaultGateways();
          track gateway.family + gateway.id
        ) {
          <span tuiTitle>
            <b>{{ gateway.family }} · {{ gateway.name }} ({{ gateway.id }})</b>
            <span tuiSubtitle>{{ gateway.wanIp || ('Unknown' | i18n) }}</span>
          </span>
        } @empty {
          <p class="g-secondary">
            {{ 'No default route is available.' | i18n }}
          </p>
        }
      }
    </section>
    @if (
      settings?.incoming &&
      configuration !== 'loading' &&
      configuration !== 'error' &&
      configuration.available
    ) {
      <section tuiCardLarge>
        <header tuiHeader="body-l">
          <h3 tuiTitle>{{ 'Incoming peer addresses' | i18n }}</h3>
        </header>
        <p class="g-secondary">
          {{
            'Enable the addresses peers should use. Address Requirements checks reachability; port forwarding uses PCP/UPnP when supported by the gateway.'
              | i18n
          }}
        </p>
        <service-interface packageId="start-os" [value]="incomingInterface()" />
      </section>
    }
  `,
  styles: `
    section + section {
      margin-block-start: 1rem;
    }

    p {
      margin: 0;
    }
  `,
  host: { class: 'g-subpage' },
  providers: [GatewayService],
  imports: [
    ReactiveFormsModule,
    RouterLink,
    DocsLinkDirective,
    i18nPipe,
    TuiButton,
    TuiCell,
    TuiLoader,
    TuiNotification,
    TuiTitle,
    TuiSwitch,
    TuiCardLarge,
    TuiHeader,
    TitleDirective,
    InterfaceComponent,
  ],
})
export default class Downloads {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly interfaces = inject(InterfaceService)
  private readonly network = toSignal(
    this.patch.watch$('serverInfo', 'network'),
  )
  private readonly packages = toSignal(this.patch.watch$('packageData'))

  protected readonly gateways = inject(GatewayService)
  protected readonly entry = DOWNLOADS_MENU_ITEM
  protected readonly config = signal<T.BitTorrentConfig | 'loading' | 'error'>(
    'loading',
  )
  protected readonly saving = signal(false)
  protected readonly controls: Record<
    keyof T.TorrentPreferences,
    FormControl<boolean>
  > = {
    downloads: new FormControl(false, { nonNullable: true }),
    seeding: new FormControl(false, { nonNullable: true }),
    incoming: new FormControl(false, { nonNullable: true }),
  }
  private readonly preferenceKeys = Object.keys(
    this.controls,
  ) as (keyof T.TorrentPreferences)[]
  protected readonly preferences = toSignal(
    this.patch
      .watch$('serverInfo', 'bittorrent')
      .pipe(tap(preferences => this.syncControls(preferences))),
  )
  protected readonly incomingInterface = computed(() => {
    const config = this.config()
    const network = this.network()
    const gateways = this.gateways.gateways()
    if (config === 'loading' || config === 'error' || !network || !gateways)
      return
    return this.interfaces
      .getServiceInterfaces(
        { hosts: { [config.interface.addressInfo.hostId]: network.host } },
        gateways,
        this.packages(),
      )
      .find(iface => iface.id === config.interface.id)
  })

  constructor() {
    for (const preference of this.preferenceKeys) {
      this.controls[preference].valueChanges
        .pipe(takeUntilDestroyed())
        .subscribe(enabled => {
          void this.save(preference, enabled)
        })
    }
    this.load()
  }

  protected async load() {
    this.config.set('loading')
    if (
      !(await this.tasks.runInline(async () => {
        this.config.set(await this.api.rpc.server.bittorrent.config({}))
      }))
    )
      this.config.set('error')
    const preferences = this.preferences()
    if (preferences) this.syncControls(preferences)
  }

  private syncControls(preferences: T.TorrentPreferences) {
    const config = this.config()
    const editable =
      config !== 'loading' &&
      config !== 'error' &&
      config.available &&
      !this.saving()
    const enabled = {
      downloads: editable,
      seeding: editable && preferences.downloads,
      incoming: editable && preferences.seeding,
    }
    for (const preference of this.preferenceKeys) {
      const control = this.controls[preference]
      control.setValue(preferences[preference], { emitEvent: false })
      if (enabled[preference]) control.enable({ emitEvent: false })
      else control.disable({ emitEvent: false })
    }
  }

  protected async save(
    preference: keyof T.TorrentPreferences,
    enabled: boolean,
  ) {
    this.saving.set(true)
    for (const preference of this.preferenceKeys)
      this.controls[preference].disable({ emitEvent: false })
    await this.tasks.run(
      () =>
        this.api.rpc.server.bittorrent.setPreferences({
          [preference]: enabled,
        }),
      'Saving',
    )
    this.saving.set(false)
    this.syncControls(this.preferences()!)
  }
}
