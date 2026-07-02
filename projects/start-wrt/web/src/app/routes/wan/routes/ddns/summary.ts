import { Component, computed } from '@angular/core'
import { TuiBadge, TuiStatus } from '@taiga-ui/kit'
import { Summary } from 'src/app/components/summary'
import { injectFormService } from 'src/app/services/form.service'
import { DdnsForm, DDNS_PROVIDERS } from './utils'
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
      }
    </section>
  `,
  imports: [TuiBadge, TuiStatus, Summary, i18nPipe],
})
export class DdnsSummary {
  protected readonly service = injectFormService<DdnsForm>()

  readonly enabled = computed(() => this.service.data()?.enabled ?? false)

  readonly providerLabel = computed(() => {
    const provider = this.service.data()?.provider
    return provider ? DDNS_PROVIDERS[provider].label : ''
  })

  readonly hostname = computed(() => this.service.data()?.fields.hostname || '')
}
