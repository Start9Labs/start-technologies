import { Component, computed, inject } from '@angular/core'
import { TuiBadge, TuiStatus } from '@taiga-ui/kit'
import { Summary } from 'src/app/components/summary'
import { injectFormService } from 'src/app/services/form.service'
import { WanIpv6Form, IPV6_LABELS } from './utils'
import { WanIpv6Service } from './service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

const SUMMARY_FIELDS = [
  'wan',
  'prefix',
  'gateway',
  'ip6prefix',
  'ip6prefixlen',
  'ip4prefixlen',
  'border',
] as const

@Component({
  selector: '[wanIpv6Summary]',
  template: `
    <section>
      @if (mode(); as m) {
        <div appSummary>
          {{ 'Status' | i18n }}
          <span tuiSubtitle [style.gap.rem]="0.375">
            @if (m === 'disabled') {
              <span tuiBadge tuiStatus appearance="neutral">
                {{ 'Disabled' | i18n }}
              </span>
            } @else {
              <span tuiBadge tuiStatus appearance="positive">
                {{ 'Enabled' | i18n }}
              </span>
              <span tuiBadge tuiStatus appearance="positive">
                {{ IPV6_LABELS[m] | i18n }}
              </span>
            }
          </span>
        </div>
      }
      @if (assignedIp(); as ip) {
        <div [appSummary]="ip">{{ 'Assigned IP' | i18n }}</div>
      }
      @for (item of items(); track item.label) {
        @if (item.val; as val) {
          <div [appSummary]="val">{{ item.label | i18n }}</div>
        }
      }
    </section>
  `,
  imports: [TuiBadge, TuiStatus, Summary, i18nPipe],
})
export class WanIpv6Summary {
  protected readonly service = injectFormService<WanIpv6Form>()
  private readonly ipv6Service = inject(WanIpv6Service)
  protected readonly IPV6_LABELS = IPV6_LABELS
  readonly assignedIp = this.ipv6Service.assignedIpv6

  readonly mode = computed(() => this.service.data()?.ip.mode)

  readonly items = computed(() => {
    const ip = this.service.data()?.ip
    if (!ip) return []

    return SUMMARY_FIELDS.map(key => ({
      label: IPV6_LABELS[key],
      val: ip[key],
    }))
  })
}
