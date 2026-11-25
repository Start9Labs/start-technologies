import { inject, Injectable, signal } from '@angular/core'
import { TuiNotificationService } from '@taiga-ui/core'
import {
  ApiService,
  SystemInfoRes,
  SystemUpdateStream,
  VersionInfo,
} from './api/api.service'
import { AuthService } from './auth.service'
import {
  ConnectionService,
  isNetworkError,
  PROBE_TIMEOUT_MS,
} from './connection.service'
import { RpcSocketService } from '@start9labs/shared'
import { StaleUiService } from './stale-ui.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'

/** sessionStorage: a version an earlier release's update page confirmed before reloading. */
const UPDATED_TO_KEY = 'startwrt.updated-to'
/** sessionStorage: a requested version that init() checks against the firmware after a reload. */
const UPDATE_TARGET_KEY = 'startwrt.update-target'

function takeSession(key: string): string | null {
  const value = sessionStorage.getItem(key)
  sessionStorage.removeItem(key)
  return value
}

@Injectable({
  providedIn: 'root',
})
export class SystemService {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly connection = inject(ConnectionService)
  private readonly sockets = inject(RpcSocketService)
  private readonly staleUi = inject(StaleUiService)
  private readonly i18n = inject(i18nPipe)
  private targetVersion = ''

  readonly info = signal<SystemInfoRes | null>(null)
  readonly newerVersions = signal<VersionInfo[]>([])
  readonly updateAvailable = signal(false)
  readonly updating = signal(false)
  readonly rebooting = signal(false)

  constructor() {
    // Idle pages need firmware checks even without form polling.
    setInterval(() => {
      if (
        document.visibilityState === 'hidden' ||
        this.updating() ||
        this.rebooting() ||
        this.connection.unreachable()
      ) {
        return
      }
      // Silent: form pollers and user actions own unreachable reporting.
      this.refresh().catch(() => {})
    }, 30_000)
  }

  async init(): Promise<void> {
    const updatedTo = takeSession(UPDATED_TO_KEY)
    if (updatedTo) this.announceUpdated(updatedTo)

    const target = takeSession(UPDATE_TARGET_KEY)
    const running = this.api.systemInfo()
    if (target) {
      running.then(
        ({ version }) =>
          version === target
            ? this.announceUpdated(target)
            : this.announceUpdateFailed(),
        () => {},
      )
    }

    try {
      const [info, newerVersions] = await Promise.all([
        running,
        this.api.systemNewerVersions(),
      ])

      this.setInfo(info)
      this.newerVersions.set(newerVersions)
      this.updateAvailable.set(newerVersions.length > 0)
    } catch (e) {
      console.error('Failed to fetch system info:', e)
    }
  }

  async refresh(): Promise<void> {
    this.setInfo(await this.api.systemInfo())
  }

  private setInfo(info: SystemInfoRes): void {
    this.info.set(info)
    this.staleUi.check(info)
  }

  async startUpdate(targetVersion: string): Promise<void> {
    this.targetVersion = targetVersion
    this.updating.set(true)
    // Mute background FormService poll errors for the whole update window —
    // the device becomes unreachable during the reboot. Released on every
    // terminal path here and in onUpdateFailed / pollForReconnect.
    this.connection.suppress()

    let stream: SystemUpdateStream
    try {
      stream = await this.api.systemUpdate({ targetVersion })
    } catch (e) {
      this.updating.set(false)
      this.connection.resume()
      throw e
    }
    const { target } = stream.initial
    if (!target) {
      this.updating.set(false)
      this.connection.resume()
      return
    }
    this.targetVersion = target
    void this.observeUpdate(stream.items)
  }

  /** A dropped stream starts recovery; a verified response confirms the update. */
  private async observeUpdate(
    items: SystemUpdateStream['items'],
  ): Promise<void> {
    try {
      for await (const _ of items);
      this.awaitReboot()
    } catch (error) {
      if (isNetworkError(error)) this.awaitReboot()
      else this.onUpdateFailed()
    }
  }

  private awaitReboot(): void {
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
    this.announceUpdateFailed()
  }

  private announceUpdated(version: string): void {
    this.alerts
      .open(fill(this.i18n.transform('Updated to v{version}'), { version }), {
        appearance: 'positive',
      })
      .subscribe()
  }

  private announceUpdateFailed(): void {
    this.alerts
      .open(this.i18n.transform('Update failed — check device logs'), {
        appearance: 'negative',
      })
      .subscribe()
  }

  private async pollForReconnect(): Promise<void> {
    const maxAttempts = 60
    // A successful update reboots the device, so a poll throws a network
    // error before it comes back; a failed update reaches here via
    // a dropped progress stream without the device ever going unreachable.
    let wentOffline = false
    for (let i = 0; i < maxAttempts; i++) {
      await new Promise(r => setTimeout(r, 5000))
      try {
        const info = await this.api.systemInfo(PROBE_TIMEOUT_MS)
        // Device is reachable again.
        this.rebooting.set(false)
        this.connection.resume()
        if (wentOffline) {
          if (info.version !== this.targetVersion) {
            this.onUpdateFailed()
            return
          }
          // It really rebooted — sysupgrade wiped the session store, so the
          // user is headed to login either way. If the new firmware ships a
          // different UI bundle, reload outright to land there on the NEW
          // bundle (index.html revalidates its ETag, so the reload is
          // guaranteed fresh); init() confirms the version after the reload.
          if (this.staleUi.isStale(info)) {
            sessionStorage.setItem(UPDATE_TARGET_KEY, this.targetVersion)
            window.location.reload()
            return
          }
          // Same-bundle update (e.g. a re-flash): confirm success, then send
          // the user to login rather than waiting for their next action to 401.
          this.announceUpdated(this.targetVersion)
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
          this.sockets.disconnect()
        }
        // A non-network error means the device responded — keep polling.
      }
    }
    this.rebooting.set(false)
    this.connection.resume()
    sessionStorage.setItem(UPDATE_TARGET_KEY, this.targetVersion)
    this.connection.reportUnreachable({
      message: this.i18n.transform(
        'The router has not come back since the update. This page reloads when it does.',
      ),
      probeUrl: window.location.origin,
      onReconnect: 'reload',
    })
  }
}
