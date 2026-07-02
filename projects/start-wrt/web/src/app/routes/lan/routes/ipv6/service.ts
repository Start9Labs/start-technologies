import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { LanIpv6Form, numberToPrefix, prefixToNumber } from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

export type LanIpv6Data = LanIpv6Form & {
  ip6addr?: string // For summary display
  wanPrefix?: number // WAN prefix for validation (read-only)
}

@Injectable()
export class LanIpv6Service extends FormService<LanIpv6Data> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  async load(): Promise<LanIpv6Data> {
    const res = await this.api.lanIpv6Get()

    return {
      strategy: {
        slaac: res.slaac,
        dhcpv6: res.dhcpv6,
      },
      subnet: {
        prefix: numberToPrefix(res.prefix),
      },
      ip6addr: res.ip6addr || '',
      wanPrefix: res.wan_prefix,
    }
  }

  async store(data: LanIpv6Data): Promise<void> {
    await this.api.lanIpv6Set({
      slaac: data.strategy.slaac,
      // DHCPv6 requires SLAAC (RA) to function
      dhcpv6: data.strategy.slaac && data.strategy.dhcpv6,
      prefix: prefixToNumber(data.subnet.prefix),
    })
  }

  override async save(data: LanIpv6Data): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Applying IPv6 settings...'),
      success: this.i18n.transform('IPv6 settings applied'),
      restart: true,
    })
  }
}
