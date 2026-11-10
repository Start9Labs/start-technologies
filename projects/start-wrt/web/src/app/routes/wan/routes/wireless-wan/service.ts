import { inject, Injectable } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiNotificationService } from '@taiga-ui/core'
import { firstValueFrom } from 'rxjs'
import { FormService } from 'src/app/services/form.service'
import {
  ApiService,
  WwanConnectRequest,
  WwanJoinOutcome,
  WwanNetwork,
  WwanPriority,
  WwanResponse,
} from 'src/app/services/api/api.service'
import { SystemService } from 'src/app/services/system.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import { WWAN_REMOTE_ACCESS } from './remote-access'
import { STOP_MESSAGES, translateUplinkErrors } from './utils'

@Injectable()
export class WwanService extends FormService<WwanResponse> {
  private readonly api = inject(ApiService)
  private readonly system = inject(SystemService)
  private readonly i18n = inject(i18nPipe)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly notifications = inject(TuiNotificationService)

  load(): Promise<WwanResponse> {
    return this.api.wwanGet()
  }

  async store(): Promise<void> {}

  /** The radio's scan results, or null when the scan failed. */
  async scan(radio: string): Promise<WwanNetwork[] | null> {
    let networks: WwanNetwork[] | null = null
    await this.actions.run(
      async () => {
        networks = await translateUplinkErrors(this.i18n, () =>
          this.api.wwanScan({ radio }),
        )
      },
      { loading: this.i18n.transform('Scanning') },
    )
    return networks
  }

  /** Resolves false when the user cancels or the join fails. */
  async connect(params: WwanConnectRequest): Promise<boolean> {
    return (
      (await this.confirmRemoteAccess()) &&
      this.join(() => this.api.wwanConnect(params))
    )
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

  private async confirmRemoteAccess(): Promise<boolean> {
    if ((this.system.info()?.remoteAccess ?? 'default') === 'never') return true
    const turnOff = await firstValueFrom(
      this.dialogs.open<boolean>(WWAN_REMOTE_ACCESS, { size: 'm' }),
      { defaultValue: null },
    )
    if (turnOff === null) return false
    return (
      !turnOff ||
      this.actions.run(async () => {
        await this.api.setPreferences({ remoteAccess: 'never' })
        await this.system.refresh()
      })
    )
  }

  private join(request: () => Promise<WwanJoinOutcome>): Promise<boolean> {
    return this.actions.run(
      async () => {
        const outcome = await translateUplinkErrors(this.i18n, request)
        await this.refreshAndWait()
        const data = this.data()
        if (outcome === 'stopped') {
          throw new Error(
            (data && this.stopMessage(data)) ??
              this.i18n.transform('The router couldn’t join the network.'),
          )
        }
        this.notifications
          .open(
            this.i18n.transform(
              outcome === 'connected'
                ? 'Joined network'
                : 'The router is still trying to join the network. The Summary shows the result.',
            ),
            { appearance: outcome === 'connected' ? 'positive' : 'info' },
          )
          .subscribe()
      },
      { loading: this.i18n.transform('Joining network'), restart: true },
    )
  }
}
