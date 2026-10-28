import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { distinctUntilChanged, map, switchMap } from 'rxjs'

import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { outboundGatewayOptions } from 'src/app/services/gateway.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import {
  outboundSettingsForm,
  outboundSettingsParams,
  outboundSettingsValue,
} from 'src/app/utils/outbound-settings'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Outbound Traffic' | i18n }}
      </div>
    </ng-container>
    <section>
      <header tuiHeader="body-l">
        <h2 tuiTitle>{{ 'Outbound Traffic' | i18n }}</h2>
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/outbound-traffic.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
      </header>
      @if (outboundForm(); as config) {
        @for (form of [config]; track form.key) {
          <app-form
            [spec]="form.spec"
            [value]="form.value"
            [buttons]="outboundButtons"
          />
        }
      }
    </section>
  `,
  styles: `
    section {
      max-inline-size: 24rem;
    }

    header {
      margin-block-end: 1rem;
    }
  `,
  host: { class: 'g-subpage' },
  imports: [
    RouterLink,
    DocsLinkDirective,
    i18nPipe,
    TuiButton,
    TuiTitle,
    TuiHeader,
    TitleDirective,
    FormComponent,
  ],
})
export default class OutboundTraffic {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly outboundForm = toSignal(
    this.patch.watch$('serverInfo', 'network').pipe(
      map(network => ({
        gateway: network.defaultOutbound,
        dns: network.outboundDns,
        options: outboundGatewayOptions(
          network.gateways,
          network.defaultOutbound,
        ),
      })),
      distinctUntilChanged((a, b) => JSON.stringify(a) === JSON.stringify(b)),
      switchMap(settings => outboundSettingsForm(this.i18n, settings, 'Auto')),
    ),
  )
  protected readonly outboundButtons = [
    {
      text: this.i18n.transform('Save'),
      handler: (input: ReturnType<typeof outboundSettingsValue>) =>
        this.tasks.run(
          async () =>
            await this.api.rpc.net.gateway.setDefaultOutbound(
              outboundSettingsParams(input),
            ),
          'Saving',
        ),
    },
  ]
}
