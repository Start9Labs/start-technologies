import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { DnsForm, dnsServersToForm, formToDnsServers } from './utils'

@Injectable()
export class DnsService extends FormService<DnsForm> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  async load(): Promise<DnsForm> {
    const res = await this.api.wanDnsGet()

    if (res.mode === 'custom' && res.servers.length > 0) {
      return {
        mode: 'custom',
        ...dnsServersToForm(res.servers),
      } as DnsForm
    }

    return {
      mode: 'isp',
      custom1: '',
      custom2: '',
      custom3: '',
      custom1Tls: false,
      custom2Tls: false,
      custom3Tls: false,
    }
  }

  async store(data: DnsForm): Promise<void> {
    if (data.mode === 'isp') {
      await this.api.wanDnsSet({ mode: 'isp' })
    } else {
      await this.api.wanDnsSet({
        mode: 'custom',
        servers: formToDnsServers(data),
      })
    }
  }

  override async save(data: DnsForm): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Applying WAN settings...'),
      success: this.i18n.transform('WAN settings applied'),
      restart: true,
    })
  }
}
