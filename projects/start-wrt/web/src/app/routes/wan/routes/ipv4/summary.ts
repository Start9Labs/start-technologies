import { Component, computed, inject } from '@angular/core'
import { Summary } from 'src/app/components/summary'
import { injectFormService } from 'src/app/services/form.service'
import { WanIpv4Form, IPV4_LABELS, netmaskFromPrefix } from './utils'
import { WanIpv4Service } from './service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

const SUMMARY_FIELDS = [
  'wan',
  'prefix',
  'mask',
  'gateway',
  'username',
  'password',
  'device',
] as const

type SummaryField = (typeof SUMMARY_FIELDS)[number]

@Component({
  selector: '[wanIpv4Summary]',
  template: `
    <section>
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
  imports: [Summary, i18nPipe],
})
export class WanIpv4Summary {
  protected readonly service = injectFormService<WanIpv4Form>()
  private readonly ipv4Service = inject(WanIpv4Service)
  readonly assignedIp = this.ipv4Service.assignedIp

  readonly items = computed(() => {
    const ip = this.service.data()?.ip
    if (!ip) return []

    return SUMMARY_FIELDS.map(key => ({
      label: IPV4_LABELS[key],
      val: this.getFieldValue(ip, key),
    }))
  })

  private getFieldValue(ip: WanIpv4Form['ip'], key: SummaryField): string {
    if (key === 'mask') {
      return netmaskFromPrefix(ip.prefix)
    }
    return ip[key as keyof typeof ip] as string
  }
}
