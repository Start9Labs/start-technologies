import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { MacForm } from './utils'

@Injectable()
export class MacService extends FormService<MacForm> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  async load(): Promise<MacForm> {
    const res = await this.api.wanMacGet()
    return {
      strategy: res.strategy,
      address: {
        mac: res.mac,
      },
    }
  }

  async store(data: MacForm): Promise<void> {
    await this.api.wanMacSet({
      strategy: data.strategy,
      mac: data.strategy === 'custom' ? data.address.mac : undefined,
    })
  }

  override async save(data: MacForm): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Applying WAN settings...'),
      success: this.i18n.transform('WAN settings applied'),
      restart: true,
    })
  }
}
