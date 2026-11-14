import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import {
  CopyService,
  DialogService,
  DocsLinkDirective,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { ISB, T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCell,
  TuiIcon,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBadge, TuiFade, TuiProgress } from '@taiga-ui/kit'
import { TuiBlockStatus } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom, map } from 'rxjs'
import {
  FormComponent,
  FormNote,
} from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import {
  clientSpec,
  clientUpsSpec,
  DEFAULT_NUT_CONFIG,
  deviceName,
  deviceServerSpec,
  NutMode,
  serverSpec,
  toClientSettings,
  toServerForm,
  toServerSettings,
} from './form'
import { NUT_MODE_PICKER } from './picker'
import { NutStatusService } from './service'
import { toUpsView } from './status'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'UPS Monitor' | i18n }}
      </div>
    </ng-container>
    @let state = statusService.state();
    @if (!connected()) {
      <tui-block-status>
        <tui-icon tuiSlot="top" icon="@tui.battery-charging" />
        <h2>{{ 'Not monitoring a UPS' | i18n }}</h2>
        {{
          'Monitor a UPS to shut down StartOS safely when its battery runs low.'
            | i18n
        }}
        <button tuiButton (click)="configure()">
          {{ 'Connect' | i18n }}
        </button>
        <a
          tuiButton
          appearance="flat-grayscale"
          iconStart="@tui.book-open-text"
          docsLink
          path="/start-os/ups-monitor.html"
        >
          {{ 'Learn more' | i18n }}
        </a>
      </tui-block-status>
    } @else {
      @let ups = view();
      @if (state.status?.monitoringError; as error) {
        <div tuiNotification appearance="negative" role="alert">
          <span tuiTitle>
            {{ 'Automatic shutdown monitoring is unavailable' | i18n }}
            <span tuiSubtitle>{{ error }}</span>
          </span>
        </div>
      }
      @if (state.error; as error) {
        <div tuiNotification appearance="negative" role="alert">
          <span tuiTitle>
            {{ 'UPS status unavailable' | i18n }}
            <span tuiSubtitle>{{ error }}</span>
          </span>
        </div>
      }

      @if (!state.status && !state.error) {
        <tui-loader size="l" />
      } @else {
        <div class="grid">
          <section class="g-card">
            <header>
              {{ 'UPS' | i18n }}
              <a
                tuiIconButton
                size="xs"
                appearance="icon"
                iconStart="@tui.book-open-text"
                docsLink
                path="/start-os/ups-monitor.html"
              >
                {{ 'Documentation' | i18n }}
              </a>
              <button
                tuiIconButton
                size="s"
                appearance="icon"
                iconStart="@tui.settings"
                (click)="configure()"
              >
                {{ 'Settings' | i18n }}
              </button>
            </header>
            @for (row of connection(); track row.key) {
              <div tuiCell="m">
                <span tuiTitle>{{ row.key | i18n }}</span>
                <span tuiTitle>{{ row.value }}</span>
              </div>
            }
            <div tuiCell="m">
              <span tuiTitle>{{ 'Shutdown protection' | i18n }}</span>
              @let protection = !!state.status && !state.status.monitoringError;
              <span
                tuiTitle
                [class._active]="protection"
                [class._unavailable]="!protection"
              >
                {{ (protection ? 'Active' : 'Unavailable') | i18n }}
              </span>
            </div>
            <div tuiCell="m">
              <span tuiTitle>{{ 'Shuts down at' | i18n }}</span>
              <span tuiTitle>
                {{ ups.shutdownCharge ?? '-' }}%
                @if (ups.shutdownRuntime !== null) {
                  / {{ duration(ups.shutdownRuntime) }}
                }
              </span>
            </div>
            @if (ups.model; as model) {
              <div tuiCell="m">
                <span tuiTitle>{{ 'Model' | i18n }}</span>
                <span tuiTitle>
                  <span tuiFade>{{ model }}</span>
                </span>
              </div>
            }
          </section>

          <section class="g-card">
            <header>{{ 'Power' | i18n }}</header>
            <div class="hero" [attr.data-appearance]="ups.power.appearance">
              <tui-icon [icon]="ups.power.icon" />
              <strong>{{ ups.power.label | i18n }}</strong>
            </div>
            @for (alert of ups.alerts; track alert) {
              <span
                tuiBadge
                appearance="negative"
                iconStart="@tui.circle-alert"
              >
                {{ alert | i18n }}
              </span>
            }
            <div tuiCell="m">
              <span tuiTitle>{{ 'Input voltage' | i18n }}</span>
              <span tuiTitle data-unit="V">{{ ups.inputVoltage ?? '-' }}</span>
            </div>
          </section>

          <section class="g-card">
            <header>{{ 'Battery' | i18n }}</header>
            <div class="hero">
              <label tuiProgressLabel>
                <tui-progress-circle
                  size="xl"
                  [max]="100"
                  [value]="ups.charge ?? 0"
                  [color]="'var(--tui-status-' + ups.batteryAppearance + ')'"
                />
                {{ ups.charge ?? '-' }}%
              </label>
              @if (ups.charging) {
                <span class="charging">{{ 'Charging' | i18n }}</span>
              }
            </div>
            <div tuiCell="m">
              <span tuiTitle>{{ 'Runtime' | i18n }}</span>
              <span tuiTitle>{{ duration(ups.runtime) }}</span>
            </div>
          </section>

          <section class="g-card">
            <header>{{ 'Load' | i18n }}</header>
            <div class="load">
              <progress
                tuiProgressBar
                [max]="100"
                [attr.value]="ups.load"
                [color]="'var(--tui-status-' + ups.loadAppearance + ')'"
              ></progress>
              <strong>{{ ups.load ?? '-' }}%</strong>
            </div>
            <div tuiCell="m">
              <span tuiTitle>{{ 'Power draw' | i18n }}</span>
              <span tuiTitle data-unit="W">{{ ups.watts ?? '-' }}</span>
            </div>
            <div tuiCell="m">
              <span tuiTitle>{{ 'Max output' | i18n }}</span>
              <span tuiTitle data-unit="W">
                {{ ups.nominalWatts ?? '-' }}
              </span>
            </div>
          </section>

          @if (sharing(); as sharing) {
            <section class="g-card clients">
              <header>{{ 'Connect other clients' | i18n }}</header>
              <p>
                {{
                  'To monitor this UPS from another StartOS server, open System > UPS Monitor there, select Connect, choose NUT client, and enter these details. Other devices take the same details in their NUT client settings.'
                    | i18n
                }}
              </p>
              <div class="details">
                @for (row of sharing; track row.key) {
                  <div tuiCell="m">
                    <span tuiTitle>{{ row.key | i18n }}</span>
                    <span tuiTitle>
                      {{ row.secret ? '••••••••' : row.value }}
                    </span>
                    <button
                      tuiIconButton
                      appearance="icon"
                      size="xs"
                      iconStart="@tui.copy"
                      (click)="copy.copy(row.value)"
                    >
                      {{ 'Copy' | i18n }}
                    </button>
                  </div>
                }
              </div>
            </section>
          }
        </div>
      }
    }
  `,
  styles: `
    :host {
      max-inline-size: 72rem;
      display: flex;
      flex-direction: column;

      > *,
      &::after {
        flex-shrink: 0;
      }
    }

    tui-block-status {
      tui-icon {
        font-size: 6rem;
        color: var(--tui-text-tertiary);
      }

      [docsLink] {
        margin-block-start: 0.75rem;
      }
    }

    [tuiNotification] {
      margin-block-end: 1rem;
    }

    tui-loader {
      flex: 1;
    }

    .g-card > header [docsLink] {
      margin-inline-start: auto;
    }

    .grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 1rem;

      :host-context(tui-root._mobile) & {
        grid-template-columns: minmax(0, 1fr);
      }
    }

    .g-card > header {
      background: transparent;
    }

    .clients {
      grid-column: 1 / -1;

      .details {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        column-gap: 2rem;

        :host-context(tui-root._mobile) & {
          grid-template-columns: minmax(0, 1fr);
        }
      }

      p {
        margin: 0 0 0.5rem;
        color: var(--tui-text-secondary);
      }
    }

    .hero {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 0.5rem;
      margin: 2rem auto;
      font: var(--tui-typography-body-l);

      tui-icon {
        font-size: 4rem;
      }

      &[data-appearance='positive'] tui-icon {
        color: var(--tui-status-positive);
      }

      &[data-appearance='warning'] tui-icon {
        color: var(--tui-status-warning);
      }

      &[data-appearance='negative'] tui-icon {
        color: var(--tui-status-negative);
      }
    }

    [tuiBadge] {
      align-self: center;
      margin-block-end: 0.5rem;
    }

    .charging {
      font: var(--tui-typography-body-s);
      color: var(--tui-text-secondary);
    }

    .load {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 1rem;
      margin: 2rem auto;
      inline-size: 80%;
      font: var(--tui-typography-body-l);

      progress {
        block-size: 1.5rem;
        border-radius: 0;
        clip-path: none;
        mask: linear-gradient(to right, #000 80%, transparent 80%);
        mask-size: 5% 100%;
      }
    }

    ._active {
      color: var(--tui-status-positive);
    }

    ._unavailable {
      color: var(--tui-status-negative);
    }

    [tuiCell] [tuiTitle] {
      &:first-child {
        color: var(--tui-text-tertiary);
      }

      &:last-child {
        flex-direction: row;
        gap: 0;
      }

      &[data-unit]::after {
        content: attr(data-unit);
        font: var(--tui-typography-ui-2xs);
        color: var(--tui-text-secondary);
      }
    }
  `,
  host: { class: 'g-subpage' },
  imports: [
    RouterLink,
    DocsLinkDirective,
    TitleDirective,
    i18nPipe,
    TuiBadge,
    TuiBlockStatus,
    TuiButton,
    TuiCell,
    TuiFade,
    TuiIcon,
    TuiLoader,
    TuiNotification,
    TuiProgress,
    TuiTitle,
  ],
  providers: [NutStatusService],
})
export default class Nut {
  private readonly dialogs = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  protected readonly statusService = inject(NutStatusService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly gateways = toSignal(
    this.patch.watch$('serverInfo', 'network', 'gateways'),
    { initialValue: {} },
  )
  protected readonly data = toSignal(
    this.patch
      .watch$('serverInfo', 'nut')
      .pipe(map(config => config ?? DEFAULT_NUT_CONFIG)),
    { initialValue: DEFAULT_NUT_CONFIG },
  )
  // Settings and Disconnect remain available when UPS readings fail.
  protected readonly connected = computed(
    () => this.data().enabled && !!this.data().settings,
  )
  protected readonly view = computed(() =>
    toUpsView(this.statusService.state().status?.variables ?? {}),
  )
  protected readonly connection = computed(
    (settings = this.data().settings): { key: i18nKey; value: string }[] =>
      settings?.mode === 'client'
        ? [
            { key: 'Role', value: this.i18n.transform('NUT client') },
            { key: 'Host', value: settings.host },
          ]
        : [
            { key: 'Role', value: this.i18n.transform('NUT server') },
            {
              key: 'Other clients can monitor',
              value: this.i18n.transform(settings?.listenAll ? 'On' : 'Off'),
            },
          ],
  )

  protected readonly copy = inject(CopyService)
  protected readonly sharing = computed(
    (
      settings = this.data().settings,
      gateways = this.gateways(),
    ): { key: i18nKey; value: string; secret?: boolean }[] | null =>
      settings?.mode === 'server' && settings.listenAll
        ? [
            ...Object.values(gateways)
              .filter(({ ipInfo }) =>
                ['ethernet', 'wireless', 'bridge'].includes(
                  ipInfo?.deviceType ?? '',
                ),
              )
              .flatMap(({ ipInfo }) => ipInfo?.subnets ?? [])
              .map(subnet => subnet.split('/')[0]!)
              .filter(address => !address.includes(':'))
              .map(value => ({ key: 'NUT server host' as i18nKey, value })),
            { key: 'NUT server port', value: '3493' },
            { key: 'UPS name', value: 'startos' },
            { key: 'Username', value: settings.remoteUsername ?? '' },
            {
              key: 'Password',
              value: settings.remotePassword ?? '',
              secret: true,
            },
          ]
        : null,
  )

  protected duration(seconds: number | null) {
    if (seconds === null) return '-'
    const minutes = Math.round(seconds / 60)
    return minutes < 60
      ? `${minutes} ${this.i18n.transform('min')}`
      : `${Math.floor(minutes / 60)} ${this.i18n.transform('h')} ${minutes % 60} ${this.i18n.transform('min')}`
  }

  protected configure() {
    const settings = this.data().settings

    if (this.connected() && settings) {
      this.open(settings.mode, settings)
    } else {
      this.dialogs
        .openComponent<NutMode>(NUT_MODE_PICKER, {
          label: 'StartOS’s Role',
          size: 'l',
        })
        .subscribe(mode => this.open(mode))
    }
  }

  private async open(mode: NutMode, settings?: T.NutSettings) {
    if (mode === 'client') {
      const current = settings?.mode === 'client' ? settings : undefined
      this.openForm(
        'NUT client',
        clientSpec(this.i18n),
        value => this.connectClient(value, current?.upsName),
        current,
      )
      return
    }

    if (settings?.mode === 'server') {
      this.openForm(
        'NUT server',
        serverSpec(this.i18n),
        value => this.save(toServerSettings(settings.device, value)),
        toServerForm(settings),
      )
      return
    }

    let devices: T.NutDevice[] = []
    const scanned = await this.tasks.run(async () => {
      devices = await this.api.rpc.server.nutScan({})
    }, 'Searching for UPS devices')

    if (!scanned) return

    const [only, ...others] = devices

    if (only && !others.length) {
      this.openForm(
        'NUT server',
        serverSpec(this.i18n),
        value => this.save(toServerSettings(only, value)),
        undefined,
        {
          text: `${this.i18n.transform('Detected')}: ${deviceName(only)}`,
          appearance: 'positive',
          top: true,
        },
      )
    } else if (only) {
      this.openForm(
        'NUT server',
        deviceServerSpec(this.i18n, devices),
        ({ device, ...value }) =>
          this.save(toServerSettings(devices[Number(device)]!, value)),
      )
    } else {
      this.dialogs
        .openAlert(
          'No UPS was found on USB. Connect the UPS’s USB cable to this server, then try again.',
          { label: 'No UPS detected' },
        )
        .subscribe()
    }
  }

  private async openForm<V>(
    label: i18nKey,
    spec: ISB.InputSpec<V>,
    submit: (value: V) => Promise<boolean>,
    value?: object,
    note?: FormNote,
  ) {
    this.formDialog.open(FormComponent, {
      label,
      data: {
        spec: await configBuilderToSpec(spec),
        value,
        note,
        buttons: [
          ...(this.connected()
            ? [
                {
                  text: this.i18n.transform('Disconnect'),
                  appearance: 'flat-destructive',
                  handler: async () =>
                    (await firstValueFrom(
                      this.dialogs.openConfirm({
                        label: 'Disconnect UPS?',
                        data: { yes: 'Disconnect', no: 'Cancel' },
                      }),
                    )) &&
                    this.tasks.run(
                      () => this.api.rpc.server.clearNut({}),
                      'Saving',
                    ),
                },
              ]
            : []),
          { text: this.i18n.transform('Save'), handler: submit },
        ],
      },
    })
  }

  private async connectClient(
    value: ReturnType<typeof clientSpec>['_TYPE'],
    current?: string,
  ) {
    let upses: T.NutRemoteUps[] = []
    const listed = await this.tasks.run(async () => {
      upses = await this.api.rpc.server.nutListUps({
        host: value.host,
        port: value.port,
      })
    }, 'Connecting to NUT server')
    const [only, ...others] = upses

    if (!listed) return false

    if (!only) {
      this.dialogs
        .openAlert('That NUT server did not report any UPS.', {
          label: 'No UPS found',
        })
        .subscribe()
      return false
    }

    if (!others.length) return this.save(toClientSettings(only.name, value))

    this.openForm(
      'NUT client',
      clientUpsSpec(this.i18n, upses, current),
      ({ upsName, ...rest }) => this.save(toClientSettings(upsName, rest)),
      value,
    )
    return true
  }

  private save(settings: T.NutSettings) {
    return this.tasks.run(
      () => this.api.rpc.server.setNut({ config: { enabled: true, settings } }),
      'Saving',
    )
  }
}
