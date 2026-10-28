import { Component, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { combineLatest, distinctUntilChanged, map, switchMap } from 'rxjs'

import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { outboundGatewayOptions } from 'src/app/services/gateway.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import {
  INACTIVE_STATUSES,
  renderPkgStatus,
} from 'src/app/services/pkg-status-rendering.service'
import {
  outboundSettingsForm,
  outboundSettingsParams,
  outboundSettingsValue,
} from 'src/app/utils/outbound-settings'

@Component({
  template: `
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
        @if (config.inactive) {
          <p tuiNotification appearance="warning">
            {{
              'Outbound settings are unavailable while this service is inactive.'
                | i18n
            }}
          </p>
        } @else {
          @for (form of [config]; track form.key) {
            <app-form
              [spec]="form.spec"
              [value]="form.value"
              [buttons]="form.buttons"
            />
          }
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
    DocsLinkDirective,
    i18nPipe,
    TuiButton,
    TuiNotification,
    TuiTitle,
    TuiHeader,
    FormComponent,
  ],
})
export default class OutboundTraffic {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  public readonly pkgId = input.required<string>()

  protected readonly outboundForm = toSignal(
    combineLatest([
      toObservable(this.pkgId).pipe(
        switchMap(id =>
          this.patch.watch$('packageData', id).pipe(map(pkg => ({ id, pkg }))),
        ),
      ),
      this.patch.watch$('serverInfo', 'network', 'gateways'),
    ]).pipe(
      map(([{ id, pkg }, gateways]) =>
        pkg
          ? {
              id,
              gateway: pkg.outboundGateway,
              dns: pkg.outboundDns,
              options: outboundGatewayOptions(gateways, pkg.outboundGateway),
              inactive: INACTIVE_STATUSES.includes(
                renderPkgStatus(pkg).primary,
              ),
            }
          : null,
      ),
      distinctUntilChanged((a, b) => JSON.stringify(a) === JSON.stringify(b)),
      switchMap(async settings => {
        if (!settings) return null
        const form = await outboundSettingsForm(
          this.i18n,
          settings,
          'Use system gateway',
        )
        return {
          ...form,
          key: JSON.stringify([settings.id, form.key]),
          inactive: settings.inactive,
          buttons: [
            {
              text: this.i18n.transform('Save'),
              handler: (input: ReturnType<typeof outboundSettingsValue>) =>
                this.tasks.run(
                  async () =>
                    await this.api.rpc.package.setOutboundGateway({
                      package: settings.id,
                      ...outboundSettingsParams(input),
                    }),
                  'Saving',
                ),
            },
          ],
        }
      }),
    ),
  )
}
