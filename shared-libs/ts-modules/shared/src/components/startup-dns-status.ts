import { Component, input } from '@angular/core'
import { T } from '@start9labs/start-core'
import { TuiNotification } from '@taiga-ui/core'
import { i18nPipe } from '../i18n/i18n.pipe'
import { DnsSourcePipe } from '../pipes/dns-source.pipe'

@Component({
  selector: 'startup-dns-status',
  template: `
    @if (status().gateway; as gateway) {
      @if (gateway.info.ipInfo; as ipInfo) {
        <p>
          <b>{{ 'Gateway' | i18n }}:</b>
          {{ gateway.info.name ?? ipInfo.name }} ({{ gateway.id }})
          <br />
          <b>{{ 'DNS source' | i18n }}:</b>
          {{ gateway.info.dns.source | dnsSource | i18n }}
          <br />
          <b>{{ 'Effective DNS servers' | i18n }}:</b>
          {{ gateway.info.dns.servers.join(', ') }}
        </p>
        @if (status().bootstrap) {
          <p>
            {{
              'StartOS uses the automatic physical gateway for DNS until stored network settings load.'
                | i18n
            }}
          </p>
        }
        @if (gateway.info.dns.source === 'public-fallback') {
          <p tuiNotification appearance="warning" role="status">
            {{
              'This gateway uses Cloudflare public DNS because it provides no usable DNS servers. Queries travel through this gateway.'
                | i18n
            }}
          </p>
        }
      }
    }
  `,
  imports: [DnsSourcePipe, i18nPipe, TuiNotification],
})
export class StartupDnsStatus {
  readonly status = input.required<T.DnsStatus>()
}
