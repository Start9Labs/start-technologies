import { inject, Injectable, signal } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { prefixFromNetmask, netmaskFromPrefix, WanIpv4Form } from './utils'

@Injectable()
export class WanIpv4Service extends FormService<WanIpv4Form> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  readonly assignedIp = signal<string | null>(null)

  async load(): Promise<WanIpv4Form> {
    const res = await this.api.wanIpv4Get()
    this.assignedIp.set(res.assigned_ip)
    return {
      ip: {
        mode: res.mode,
        wan: res.address || '',
        prefix: res.netmask ? prefixFromNetmask(res.netmask) : '',
        gateway: res.gateway || '',
        username: res.username || '',
        password: res.password || '',
        device: res.device || '',
      },
    }
  }

  async store(data: WanIpv4Form): Promise<void> {
    const { ip } = data
    await this.api.wanIpv4Set({
      mode: ip.mode,
      address: ip.mode === 'static' ? ip.wan : undefined,
      netmask: ip.mode === 'static' ? netmaskFromPrefix(ip.prefix) : undefined,
      gateway: ip.mode === 'static' ? ip.gateway : undefined,
      username: ip.mode === 'pppoe' ? ip.username : undefined,
      password: ip.mode === 'pppoe' ? ip.password : undefined,
      device: ip.mode === 'pppoe' && ip.device ? ip.device : undefined,
    })
  }

  override async save(data: WanIpv4Form): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Applying WAN settings...'),
      success: this.i18n.transform('WAN settings applied'),
      restart: true,
    })
  }
}
