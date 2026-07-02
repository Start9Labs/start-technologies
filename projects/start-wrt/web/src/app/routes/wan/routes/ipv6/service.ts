import { inject, Injectable, signal } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { WanIpv6Form } from './utils'

@Injectable()
export class WanIpv6Service extends FormService<WanIpv6Form> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  readonly assignedIpv6 = signal<string | null>(null)

  async load(): Promise<WanIpv6Form> {
    const res = await this.api.wanIpv6Get()
    this.assignedIpv6.set(res.assigned_ipv6 ?? null)
    return {
      ip: {
        mode: res.mode,
        wan: res.address || '',
        prefix: res.prefix || '',
        gateway: res.gateway || '',
        lan_prefix: res.lan_prefix || '',
        ip6prefix: res.ip6prefix || '',
        ip6prefixlen: res.ip6prefixlen || '',
        ip4prefixlen: res.ip4prefixlen || '',
        border: res.border_relay || '',
      },
    }
  }

  async store(data: WanIpv6Form): Promise<void> {
    const { ip } = data
    await this.api.wanIpv6Set({
      mode: ip.mode,
      address: ip.mode === 'static' ? ip.wan : undefined,
      prefix: ip.mode === 'static' ? ip.prefix : undefined,
      gateway: ip.mode === 'static' ? ip.gateway : undefined,
      lan_prefix:
        ip.mode === 'static' && ip.lan_prefix ? ip.lan_prefix : undefined,
      ip6prefix: ip.mode === '6rd' ? ip.ip6prefix : undefined,
      ip6prefixlen: ip.mode === '6rd' ? ip.ip6prefixlen : undefined,
      ip4prefixlen: ip.mode === '6rd' ? ip.ip4prefixlen : undefined,
      border_relay: ip.mode === '6rd' ? ip.border : undefined,
    })
  }

  override async save(data: WanIpv6Form): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Applying WAN settings...'),
      success: this.i18n.transform('WAN settings applied'),
      restart: true,
    })
  }
}
