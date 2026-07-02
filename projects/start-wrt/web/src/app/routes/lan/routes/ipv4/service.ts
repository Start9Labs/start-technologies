import { inject, Injectable } from '@angular/core'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { FormService } from 'src/app/services/form.service'
import { ApiService } from 'src/app/services/api/api.service'
import { isNetworkError } from 'src/app/services/connection.service'
import { pauseFor } from '@start9labs/shared'
import { buildRouterIp, LanIpv4Form, parseIpToForm } from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

const RESTART_TIMEOUT_MS = 60_000

@Injectable()
export class LanIpv4Service extends FormService<LanIpv4Form> {
  private readonly api = inject(ApiService)
  private readonly notifications = inject(TuiNotificationMiddleService)
  private readonly i18n = inject(i18nPipe)

  async load(): Promise<LanIpv4Form> {
    const res = await this.api.lanIpv4Get()
    return parseIpToForm(res.address)
  }

  async store(data: LanIpv4Form): Promise<void> {
    await this.api.lanIpv4Set({ address: buildRouterIp(data.ip) })
  }

  override async save(data: LanIpv4Form): Promise<boolean> {
    return this.actions.run(() => this.store(data), {
      loading: this.i18n.transform('Applying LAN settings...'),
      success: this.i18n.transform('LAN settings applied'),
      restart: true,
    })
  }

  /**
   * Save LAN IPv4 settings when the IP is changing.
   * Shows a loading notification. Re-throws VPN errors for the caller
   * to handle via a confirmation dialog. Swallows network errors
   * (expected during the network restart) — the caller owns the reconnect by
   * navigating to the new address.
   */
  async saveForIpChange(data: LanIpv4Form, force?: boolean): Promise<void> {
    const loading = this.notifications
      .open(this.i18n.transform('Applying LAN settings...'))
      .subscribe()
    // Suppress the global indicator BEFORE the request: this set restarts the
    // network and the connection drops mid-flight, so a background poller would
    // otherwise pop the generic "Reconnecting" toast in the window before the
    // caller's foreground-nav dialog takes over. On the expected network drop we
    // STAY suppressed (the caller navigates to the new IP and owns the
    // reconnect); any other error means the change was rejected with no drop, so
    // resume.
    this.connection.suppress()
    try {
      await Promise.race([
        this.api.lanIpv4Set({ address: buildRouterIp(data.ip), force }),
        pauseFor(RESTART_TIMEOUT_MS).then(() => {
          throw Object.assign(new Error('Network timeout'), { code: 0 })
        }),
      ])
    } catch (e: any) {
      if (isNetworkError(e)) return
      this.connection.resume()
      throw e
    } finally {
      loading.unsubscribe()
    }
  }
}
