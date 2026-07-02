import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { DdnsForm } from './utils'

@Injectable()
export class DdnsService extends FormService<DdnsForm> {
  private readonly api = inject(ApiService)

  async load(): Promise<DdnsForm> {
    const res = await this.api.wanDdnsGet()
    return {
      enabled: res.enabled,
      provider: res.provider as DdnsForm['provider'],
      fields: {
        username: res.username || '',
        password: res.password || '',
        hostname: res.hostname || '',
        token: res.token || '',
        zone: res.zone || '',
      },
    }
  }

  async store(data: DdnsForm): Promise<void> {
    await this.api.wanDdnsSet({
      enabled: data.enabled,
      provider: data.provider,
      hostname: data.fields.hostname || undefined,
      username: data.fields.username || undefined,
      password: data.fields.password || undefined,
      token: data.fields.token || undefined,
      zone: data.fields.zone || undefined,
    })
  }
}
