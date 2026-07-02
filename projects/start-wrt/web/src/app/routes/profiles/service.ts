import { inject, Injectable } from '@angular/core'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  ApiService,
  ProfileCreateInput,
  ProfileIdOpt,
  ProfileUpdateInput,
  ScheduleWindow,
  SecurityProfile,
} from 'src/app/services/api/api.service'
import { FormService } from 'src/app/services/form.service'
import { isNetworkError } from 'src/app/services/connection.service'
import { pauseFor } from '@start9labs/shared'

const RESTART_TIMEOUT_MS = 60_000

@Injectable({ providedIn: 'root' })
export class ProfilesService extends FormService<SecurityProfile[]> {
  private readonly api = inject(ApiService)
  private readonly notifications = inject(TuiNotificationMiddleService)
  private readonly i18n = inject(i18nPipe)

  async load() {
    // Get a list of profile IDs
    const profileIds = await this.api.profilesList()

    // Fetch full profile data for each ID
    const profiles = await Promise.all(
      profileIds.map(id => this.api.profileGet(id)),
    )

    return profiles
  }

  async store(_data: SecurityProfile[]) {
    // Not used - the individual methods below handle persistence
  }

  async createProfile(
    params: ProfileCreateInput,
    windows: ScheduleWindow[] = [],
  ) {
    await this.actions.run(
      async () => {
        const id = await this.api.profileCreate(params)
        if (windows.length) {
          await this.api.profileScheduleSet({
            interface: id.interface,
            windows,
          })
        }
        this.refresh()
      },
      {
        loading: this.i18n.transform(
          'Creating profile and restarting network...',
        ),
        success: this.i18n.transform('Profile created'),
        restart: true,
      },
    )
  }

  /**
   * @returns true if the admin IP changed (caller handles redirect/reconnect)
   * @throws error with 'VPN client' in message when IP change would break VPN peers
   */
  async updateProfile(
    params: ProfileUpdateInput,
    oldGatewayIp?: string,
    windows: ScheduleWindow[] = [],
  ): Promise<boolean> {
    const adminIpChanged =
      params.owns_lan && !!oldGatewayIp && oldGatewayIp !== params.gateway_ip

    if (adminIpChanged) {
      // Schedule write doesn't restart the network — land it before the IP
      // change, so it survives the redirect to the new admin address.
      await this.api.profileScheduleSet({
        interface: params.interface,
        windows,
      })

      const loading = this.notifications
        .open(this.i18n.transform('Applying profile settings...'))
        .subscribe()
      // Suppress the global indicator BEFORE the request: this update restarts
      // the network and the admin's connection drops mid-flight, so a background
      // poller would otherwise pop the generic "Reconnecting" toast in the
      // window before the caller's foreground-nav dialog takes over. On the
      // expected drop we STAY suppressed (the caller navigates to the new
      // address and owns the reconnect); any other error means no drop, resume.
      this.connection.suppress()
      try {
        await Promise.race([
          this.api.profileUpdate(params),
          pauseFor(RESTART_TIMEOUT_MS).then(() => {
            throw Object.assign(new Error('Network timeout'), { code: 0 })
          }),
        ])
      } catch (e: any) {
        // Network drop is expected; the caller owns the reconnect via foreground
        // navigation. Signal the admin-IP change to the caller.
        if (isNetworkError(e)) return true
        this.connection.resume()
        throw e
      } finally {
        loading.unsubscribe()
      }
      return true
    }

    await this.actions.run(
      async () => {
        await this.api.profileUpdate(params)
        await this.api.profileScheduleSet({
          interface: params.interface,
          windows,
        })
        this.refresh()
      },
      {
        loading: this.i18n.transform('Applying profile settings...'),
        success: this.i18n.transform('Profile updated'),
        restart: true,
      },
    )

    return false
  }

  async deleteProfile(params: ProfileIdOpt) {
    await this.actions.run(
      async () => {
        await this.api.profileDelete(params)
        this.refresh()
      },
      {
        loading: this.i18n.transform(
          'Deleting profile and restarting network...',
        ),
        success: this.i18n.transform('Profile deleted'),
        restart: true,
      },
    )
  }
}
