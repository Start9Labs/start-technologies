import { inject, Injectable } from '@angular/core'
import { TuiNotificationService } from '@taiga-ui/core'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { pauseFor } from '@start9labs/shared'
import {
  ConnectionService,
  isNetworkError,
  OnReconnect,
} from 'src/app/services/connection.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

// Bound on how long we wait for a restart action to return before assuming the
// connection dropped. The global indicator (driven by the background pollers)
// surfaces "Reconnecting" within a poll interval regardless; this only caps how
// long the success toast is deferred when the action's own response is wedged.
const RESTART_TIMEOUT_MS = 60_000

export interface ActionOptions {
  loading: string
  success: string
  fail: string
  /** This action restarts the network; expect a transient disconnection. */
  restart: boolean
  /** Subtitle for the reconnect indicator (e.g. the SSID to rejoin). */
  reconnectMessage: string
  /** What to do once the router is reachable again (default: 'none'). */
  onReconnect: OnReconnect
}

@Injectable({ providedIn: 'root' })
export class ActionService {
  private readonly alerts = inject(TuiNotificationService)
  private readonly notifications = inject(TuiNotificationMiddleService)
  private readonly connection = inject(ConnectionService)
  private readonly i18n = inject(i18nPipe)

  async run(
    action: () => Promise<unknown>,
    options: Partial<ActionOptions> = {},
  ): Promise<boolean> {
    const loading = this.notifications
      .open(options.loading ?? this.i18n.transform('Saving'))
      .subscribe()

    const disruption = {
      message: options.reconnectMessage,
      onReconnect: options.onReconnect,
    }

    try {
      if (options.restart) {
        // Arm the indicator's context up front so that whichever path observes
        // the drop — this action or a background poller — shows the right copy.
        this.connection.expectDisruption(disruption)
      }

      try {
        await (options.restart
          ? Promise.race([
              action(),
              pauseFor(RESTART_TIMEOUT_MS).then(() => {
                throw Object.assign(new Error('Network timeout'), { code: 0 })
              }),
            ])
          : action())
      } catch (e: any) {
        if (!isNetworkError(e)) throw e

        // Network error: the global indicator owns recovery — never surface a
        // raw "Unknown Error" toast. For a restart action the write landed
        // before the drop, so hand the success copy to the indicator and let it
        // confirm AFTER the router answers again (recover()), rather than toast
        // success now while still unreachable. An unexpected drop on a
        // non-restart action is a plain failure.
        this.connection.reportUnreachable({
          ...disruption,
          successMessage: options.restart ? options.success : undefined,
        })
        return !!options.restart
      }

      if (options.success) {
        this.alerts
          .open(options.success, { appearance: 'positive' })
          .subscribe()
      }

      return true
    } catch (e: any) {
      console.error(e)
      this.alerts
        .open(e.message || options.fail || e, { appearance: 'negative' })
        .subscribe()

      return false
    } finally {
      loading.unsubscribe()
    }
  }
}
