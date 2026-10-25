import { DatePipe } from '@angular/common'
import { Component, computed } from '@angular/core'
import { TuiBadge, TuiStatus } from '@taiga-ui/kit'
import { Summary } from 'src/app/components/summary'
import { injectFormService } from 'src/app/services/form.service'
import { DdnsData } from './service'
import { DDNS_PROVIDERS } from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[ddnsSummary]',
  template: `
    <section>
      <div appSummary>
        {{ 'Status' | i18n }}
        <span tuiSubtitle>
          @if (enabled()) {
            <span tuiBadge tuiStatus appearance="positive">
              {{ 'Enabled' | i18n }}
            </span>
          } @else {
            <span tuiBadge tuiStatus appearance="neutral">
              {{ 'Disabled' | i18n }}
            </span>
          }
        </span>
      </div>
      @if (enabled()) {
        <div [appSummary]="providerLabel()">{{ 'Provider' | i18n }}</div>
        @if (hostname(); as hostname) {
          <div [appSummary]="hostname">{{ 'Hostname' | i18n }}</div>
        }
        @if (service.data()?.certificate; as certificate) {
          <div appSummary>
            {{ 'Certificate' | i18n }}
            <span tuiSubtitle>
              @switch (certificate.state) {
                @case ('issued') {
                  <span tuiBadge tuiStatus appearance="positive">
                    {{ 'Valid until' | i18n }}
                    {{ certificate.expires | date: 'mediumDate' }}
                  </span>
                }
                @case ('failed') {
                  <span tuiBadge tuiStatus appearance="negative">
                    {{ 'Failed' | i18n }}
                  </span>
                  {{ certificate.reason }}
                }
                @default {
                  <span tuiBadge tuiStatus appearance="neutral">
                    {{ 'Pending' | i18n }}
                  </span>
                }
              }
            </span>
          </div>
        }
      }
    </section>
  `,
  imports: [DatePipe, TuiBadge, TuiStatus, Summary, i18nPipe],
})
export class DdnsSummary {
  protected readonly service = injectFormService<DdnsData>()

  readonly enabled = computed(() => this.service.data()?.enabled ?? false)

  readonly providerLabel = computed(() => {
    const provider = this.service.data()?.provider
    return provider ? (DDNS_PROVIDERS[provider]?.label ?? provider) : ''
  })

  readonly hostname = computed(() => this.service.data()?.fields.hostname || '')
}
