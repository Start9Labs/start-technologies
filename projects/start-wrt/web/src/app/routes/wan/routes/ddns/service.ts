import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { AcmeCertificateStatus } from 'src/app/services/api/bindings'
import { DdnsForm } from './utils'

export type DdnsData = DdnsForm & {
  certificate?: AcmeCertificateStatus | null
}

@Injectable()
export class DdnsService extends FormService<DdnsData> {
  private readonly api = inject(ApiService)

  async load(): Promise<DdnsData> {
    const res = await this.api.wanDdnsGet()
    return {
      enabled: res.enabled,
      provider: res.provider as DdnsForm['provider'],
      acme: res.acme,
      certificate: res.certificate,
      fields: {
        username: res.username || '',
        password: res.password || '',
        hostname: res.hostname || '',
        token: res.token || '',
        zone: res.zone || '',
      },
    }
  }

  async store(data: DdnsData): Promise<void> {
    await this.api.wanDdnsSet({
      enabled: data.enabled,
      provider: data.provider,
      acme: data.acme,
      hostname: data.fields.hostname || undefined,
      username: data.fields.username || undefined,
      password: data.fields.password || undefined,
      token: data.fields.token || undefined,
      zone: data.fields.zone || undefined,
    })
  }
}
