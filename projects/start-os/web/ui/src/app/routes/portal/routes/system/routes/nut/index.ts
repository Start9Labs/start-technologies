import { KeyValuePipe } from '@angular/common'
import { Component, computed, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCell,
  TuiIcon,
  TuiLoader,
  TuiNotificationService,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiTooltip } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { map } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import {
  DEFAULT_NUT_CONFIG,
  nutSpec,
  NutForm,
  toNutConfig,
  toNutForm,
} from './form'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Network UPS Tools' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/network-ups-tools.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>

    <section class="g-card">
      <header>
        {{ 'Network UPS Tools' | i18n }}
        <tui-icon
          [tuiTooltip]="
            'NUT shuts down StartOS when the UPS battery is low' | i18n
          "
        />
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/network-ups-tools.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button
          tuiIconButton
          size="xs"
          type="button"
          iconStart="@tui.settings"
          [style.margin-inline-start]="'auto'"
          (click)="configure()"
        >
          {{ 'Configure' | i18n }}
        </button>
        <button
          tuiIconButton
          size="xs"
          type="button"
          iconStart="@tui.refresh-cw"
          [disabled]="!data().enabled || !data().settings"
          [loading]="refreshing()"
          (click)="refreshStatus()"
        >
          {{ 'Refresh' | i18n }}
        </button>
      </header>

      @if (status()?.target || target(); as url) {
        <div tuiCell>
          <div tuiTitle>
            <div tuiSubtitle>
              <code>{{ 'Configured target' | i18n }}</code>
            </div>
            <b>{{ url }}</b>
          </div>
        </div>
      }
      @if (!data().enabled) {
        <app-placeholder icon="@tui.zap">
          {{ 'Network UPS Tools is disabled' | i18n }}
        </app-placeholder>
      } @else if (status(); as current) {
        @for (row of current.variables | keyvalue; track row.key) {
          <div tuiCell>
            <div tuiTitle>
              <div tuiSubtitle>
                <code>{{ row.key }}</code>
              </div>
              <b>{{ row.value }}</b>
            </div>
          </div>
        }
      } @else if (refreshing()) {
        <tui-loader [style.height.rem]="5" />
      } @else {
        <app-placeholder icon="@tui.zap">
          {{ 'No UPS data available' | i18n }}
        </app-placeholder>
      }
    </section>
  `,
  styles: `
    :host {
      max-width: 36rem;
    }
  `,
  imports: [
    KeyValuePipe,
    RouterLink,
    DocsLinkDirective,
    TitleDirective,
    i18nPipe,
    TuiButton,
    TuiButtonLoading,
    TuiCell,
    TuiIcon,
    TuiLoader,
    TuiTitle,
    TuiTooltip,
    PlaceholderComponent,
  ],
})
export default class Nut {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly formDialog = inject(FormDialogService)
  private readonly tasks = inject(TaskService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private refreshGeneration = 0

  protected readonly status = signal<T.NutStatus | null>(null)
  protected readonly refreshing = signal(false)
  protected readonly data = toSignal(
    this.patch
      .watch$('serverInfo', 'nut')
      .pipe(map(config => config ?? DEFAULT_NUT_CONFIG)),
    { initialValue: DEFAULT_NUT_CONFIG },
  )
  protected readonly target = computed(() => {
    const config = this.data()
    if (!config.enabled || !config.settings) return ''

    switch (config.settings.mode) {
      case 'server':
        return `${config.settings.upsName}@localhost:3493`
      case 'client':
        return `${config.settings.upsName}@${config.settings.host}:${config.settings.port}`
    }
  })

  private readonly refreshOnConfigChange = effect(() => {
    void this.refreshStatus(this.data())
  })

  protected async refreshStatus(config = this.data()): Promise<void> {
    const generation = ++this.refreshGeneration
    this.status.set(null)

    if (!config.enabled || !config.settings) {
      this.refreshing.set(false)
      return
    }

    this.refreshing.set(true)

    try {
      const status = await this.api.getNutStatus({})
      if (generation === this.refreshGeneration) this.status.set(status)
    } catch (error: unknown) {
      if (generation === this.refreshGeneration) {
        this.showStatusAlert(error)
      }
    } finally {
      if (generation === this.refreshGeneration) this.refreshing.set(false)
    }
  }

  protected async configure(): Promise<void> {
    this.formDialog.open(FormComponent, {
      label: this.i18n.transform('Network UPS Tools'),
      data: {
        spec: await configBuilderToSpec(nutSpec(this.i18n)),
        value: toNutForm(this.data()),
        buttons: [
          {
            text: this.i18n.transform('Save'),
            handler: (value: NutForm) =>
              this.tasks.run(
                () => this.api.setNut({ config: toNutConfig(value) }),
                'Saving',
              ),
          },
        ],
      },
    })
  }

  private showStatusAlert(error: unknown): void {
    const detail =
      typeof error === 'string'
        ? error
        : error &&
            typeof error === 'object' &&
            'message' in error &&
            typeof error.message === 'string'
          ? error.message
          : this.i18n.transform('Unknown error')

    this.alerts
      .open(
        `${this.i18n.transform(
          'StartOS could not read UPS status with the current NUT configuration.',
        )}\n\n${this.i18n.transform(
          'The UPS did not return status data. Check the UPS name, host, port, credentials, and driver.',
        )}\n\n${detail}`,
        {
          label: this.i18n.transform('UPS status unavailable'),
          appearance: 'negative',
        },
      )
      .subscribe()
  }
}
