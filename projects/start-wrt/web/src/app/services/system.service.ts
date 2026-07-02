import { inject, Injectable, signal } from '@angular/core'
import { TuiNotificationService } from '@taiga-ui/core'
import {
  ApiService,
  FullProgress,
  SystemInfoRes,
  VersionInfo,
} from './api/api.service'
import { AuthService } from './auth.service'
import { ConnectionService, isNetworkError } from './connection.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Injectable({
  providedIn: 'root',
})
export class SystemService {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly connection = inject(ConnectionService)
  private readonly i18n = inject(i18nPipe)
  private ws: WebSocket | null = null
  private targetVersion = ''

  readonly info = signal<SystemInfoRes | null>(null)
  readonly newerVersions = signal<VersionInfo[]>([])
  readonly updateAvailable = signal(false)
  readonly updating = signal(false)
  readonly rebooting = signal(false)

  async init(): Promise<void> {
    try {
      const [info, newerVersions] = await Promise.all([
        this.api.systemInfo(),
        this.api.systemNewerVersions(),
      ])

      this.info.set(info)
      this.newerVersions.set(newerVersions)
      this.updateAvailable.set(newerVersions.length > 0)
    } catch (e) {
      console.error('Failed to fetch system info:', e)
    }
  }

  async refresh(): Promise<void> {
    const info = await this.api.systemInfo()
    this.info.set(info)
  }

  async startUpdate(targetVersion: string): Promise<void> {
    this.targetVersion = targetVersion
    this.updating.set(true)
    // Mute background FormService poll errors for the whole update window —
    // the device becomes unreachable during the reboot. Released on every
    // terminal path here and in onUpdateFailed / pollForReconnect.
    this.connection.suppress()

    try {
      const { progress } = await this.api.systemUpdate({
        targetVersion,
      })

      if (progress) {
        this.subscribeToProgress(progress)
      } else {
        // No progress channel returned — nothing to stream; clear the
        // updating state so the progress dialog doesn't hang open.
        this.updating.set(false)
        this.connection.resume()
      }
    } catch (e) {
      this.updating.set(false)
      this.connection.resume()
      throw e
    }
  }

  private subscribeToProgress(guid: string): void {
    const protocol = location.protocol === 'http:' ? 'ws' : 'wss'
    const ws = new WebSocket(`${protocol}://${location.host}/ws/rpc/${guid}`)

    ws.onmessage = event => {
      try {
        const progress = JSON.parse(event.data) as FullProgress
        if (progress.overall === true) {
          ws.close()
          this.onUpdateComplete()
        } else if (progress.overall === false) {
          ws.close()
          this.onUpdateFailed()
        }
      } catch {
        // Ignore parse errors
      }
    }

    ws.onclose = () => {
      this.ws = null
      // Still "updating" means the socket dropped without a terminal progress
      // message — an explicit failure already cleared `updating` via
      // onUpdateFailed, so reaching here means the device is rebooting.
      if (this.updating()) {
        this.onUpdateComplete()
      }
    }

    ws.onerror = () => {
      // onerror is always followed by onclose, so let onclose handle cleanup
    }

    this.ws = ws
  }

  private onUpdateComplete(): void {
    this.updating.set(false)
    this.rebooting.set(true)
    this.pollForReconnect()
  }

  private onUpdateFailed(): void {
    // A clean failure never reboots the device. Clearing `updating` (and
    // leaving `rebooting` false) lets the progress dialog close without a
    // spurious "Device is restarting…" flash; surface the error as a toast.
    this.updating.set(false)
    this.connection.resume()
    this.alerts
      .open(this.i18n.transform('Update failed — check device logs'), {
        appearance: 'negative',
      })
      .subscribe()
  }

  private async pollForReconnect(): Promise<void> {
    const maxAttempts = 60 // ~5 minutes with 5s interval
    // A successful update reboots the device, so a poll throws a network
    // error before it comes back; a failed update reaches here via
    // ws.onclose without the device ever going unreachable.
    let wentOffline = false
    for (let i = 0; i < maxAttempts; i++) {
      await new Promise(r => setTimeout(r, 5000))
      try {
        await this.api.systemInfo()
        // Device is reachable again.
        this.rebooting.set(false)
        this.connection.resume()
        if (wentOffline) {
          // It really rebooted — sysupgrade wiped the session store. Confirm
          // success, then send the user to login rather than waiting for
          // their next action to 401.
          this.alerts
            .open(`Updated to v${this.targetVersion}`, {
              appearance: 'positive',
            })
            .subscribe()
          this.auth.setUnverified()
        } else {
          // Never lost contact — the update failed before any reboot and the
          // session is still valid. Just refresh.
          await this.init()
        }
        return
      } catch (e) {
        if (isNetworkError(e)) {
          wentOffline = true
        }
        // A non-network error means the device responded — keep polling.
      }
    }
    // Timed out — user will need to refresh manually
    this.rebooting.set(false)
    this.connection.resume()
  }
}
