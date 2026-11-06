import { inject, Injectable } from '@angular/core'
import { FormService } from 'src/app/services/form.service'
import {
  ApiService,
  WwanConnectRequest,
  WwanPriority,
  WwanResponse,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Injectable()
export class WwanService extends FormService<WwanResponse> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  load(): Promise<WwanResponse> {
    return this.api.wwanGet()
  }

  async store(): Promise<void> {}

  // Joining reloads the network and restarts Wi-Fi on the uplink's band.
  connect(params: WwanConnectRequest): Promise<boolean> {
    return this.actions.run(
      async () => {
        await this.api.wwanConnect(params)
        await this.refreshAndWait()
      },
      {
        loading: this.i18n.transform('Joining network'),
        success: this.i18n.transform('Joined network'),
        restart: true,
      },
    )
  }

  setPriority(priority: WwanPriority): Promise<boolean> {
    return this.actions.run(async () => {
      await this.api.wwanPrioritySet({ priority })
      await this.refreshAndWait()
    })
  }

  disconnect(): Promise<boolean> {
    return this.actions.run(
      async () => {
        await this.api.wwanDisconnect()
        await this.refreshAndWait()
      },
      {
        loading: this.i18n.transform('Removing uplink'),
        success: this.i18n.transform('Uplink removed'),
        restart: true,
      },
    )
  }
}
