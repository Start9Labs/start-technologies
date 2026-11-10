import { inject, Injectable } from '@angular/core'
import { TuiNotificationService } from '@taiga-ui/core'
import { FormService } from 'src/app/services/form.service'
import {
  ApiService,
  WwanConnectRequest,
  WwanJoinOutcome,
  WwanPriority,
  WwanResponse,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import { STOP_MESSAGES } from './utils'

@Injectable()
export class WwanService extends FormService<WwanResponse> {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly notifications = inject(TuiNotificationService)

  load(): Promise<WwanResponse> {
    return this.api.wwanGet()
  }

  async store(): Promise<void> {}

  // Joining reloads the network and restarts Wi-Fi on the uplink's band.
  connect(params: WwanConnectRequest): Promise<boolean> {
    return this.join(() => this.api.wwanConnect(params))
  }

  reconnect(): Promise<boolean> {
    return this.join(() => this.api.wwanReconnect())
  }

  /** The stopped uplink's reason, for the user. */
  stopMessage({ uplink }: WwanResponse): string | null {
    return uplink?.stopped
      ? fill(this.i18n.transform(STOP_MESSAGES[uplink.stopped]), {
          ssid: uplink.ssid,
        })
      : null
  }

  private join(request: () => Promise<WwanJoinOutcome>): Promise<boolean> {
    return this.actions.run(
      async () => {
        const outcome = await request()
        await this.refreshAndWait()
        const data = this.data()
        const stopped = data && this.stopMessage(data)
        if (outcome === 'stopped' && stopped) throw new Error(stopped)
        if (outcome === 'connected') {
          this.notifications
            .open(this.i18n.transform('Joined network'), {
              appearance: 'positive',
            })
            .subscribe()
        }
      },
      { loading: this.i18n.transform('Joining network'), restart: true },
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
