import { KeyValuePipe } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiLoader, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader, TuiList } from '@taiga-ui/layout'
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
import { NutStatusService } from './service'

@Component({
  template: `
    @let state = statusService.state();

    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Network UPS Tools' | i18n }}
      </div>
    </ng-container>

    <section tuiCardLarge="compact" appearance="floating">
      <header tuiHeader>
        <hgroup tuiTitle>
          <h2>{{ 'Network UPS Tools' | i18n }}</h2>
          <p tuiSubtitle>
            {{ 'NUT shuts down StartOS when the UPS battery is low' | i18n }}
          </p>
        </hgroup>
        <aside tuiAccessories>
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
            [loading]="state.refreshing"
            (click)="statusService.refresh()"
          >
            {{ 'Refresh' | i18n }}
          </button>
        </aside>
      </header>

      @if (state.error; as error) {
        <div tuiNotification appearance="negative" role="alert">
          <span tuiTitle>
            {{ 'UPS status unavailable' | i18n }}
            <span tuiSubtitle>{{ error }}</span>
          </span>
        </div>
      } @else if (state.status?.monitoringError; as error) {
        <div tuiNotification appearance="negative" role="alert">
          <span tuiTitle>
            {{ 'Automatic shutdown monitoring is unavailable' | i18n }}
            <span tuiSubtitle>{{ error }}</span>
          </span>
        </div>
      }

      @if (state.status?.target || target(); as url) {
        <dl tuiList="s">
          <dt>{{ 'Configured target' | i18n }}</dt>
          <dd>
            <code>{{ url }}</code>
          </dd>
          @if (state.status; as current) {
            @for (row of current.variables | keyvalue; track row.key) {
              <dt>
                <code>{{ row.key }}</code>
              </dt>
              <dd>{{ row.value }}</dd>
            }
          }
        </dl>
      }
      @if (!data().enabled) {
        <app-placeholder icon="@tui.zap">
          {{ 'Network UPS Tools is disabled' | i18n }}
        </app-placeholder>
      } @else if (state.refreshing && !state.status) {
        <tui-loader [style.height.rem]="5" />
      } @else if (!state.status) {
        <app-placeholder icon="@tui.zap">
          {{ 'No UPS data available' | i18n }}
        </app-placeholder>
      }
    </section>
  `,
  styles: `
    header {
      flex-wrap: wrap;
    }

    aside {
      margin-inline-start: auto;
    }

    dt,
    dd {
      min-inline-size: 0;
      overflow-wrap: anywhere;
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
    TuiCardLarge,
    TuiHeader,
    TuiList,
    TuiLoader,
    TuiNotification,
    TuiTitle,
    PlaceholderComponent,
  ],
  providers: [NutStatusService],
})
export default class Nut {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly formDialog = inject(FormDialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  protected readonly statusService = inject(NutStatusService)
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
      case 'client': {
        const { host, port, upsName } = config.settings
        return `${upsName}@${host.includes(':') && !host.startsWith('[') ? `[${host}]` : host}:${port}`
      }
    }
  })

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
                () =>
                  this.api.rpc.server.setNut({ config: toNutConfig(value) }),
                'Saving',
              ),
          },
        ],
      },
    })
  }
}
