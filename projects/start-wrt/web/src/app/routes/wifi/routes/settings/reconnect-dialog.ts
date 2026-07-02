import { Component, inject, OnDestroy, OnInit, signal } from '@angular/core'
import {
  TuiDialogContext,
  TuiLoader,
  TuiNotificationService,
} from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  ConnectionService,
  isNetworkError,
} from 'src/app/services/connection.service'
import { IS_MOCK } from 'src/app/utils/workspace-config'

// Per-probe timeout. Crucially this CANCELS the in-flight request (Angular
// aborts the XHR when the RxJS timeout unsubscribes), so the wedged socket the
// WiFi restart leaves behind is torn down rather than stalling for the full TCP
// timeout. Mirrors ConnectionService's probe.
const PROBE_TIMEOUT_MS = 5000
const POLL_INTERVAL_MS = 3000
// Defense-in-depth: this dialog is closable:false / dismissible:false, so it
// must never be able to stay open indefinitely. Mirrors the old
// ReconnectingDialog's 120s net. Covers the residual sliver where wifiSet
// rejects as a network drop (→ resolves false) yet the dialog's own probes
// never observe the drop, so neither ssidConfirmed nor networkDropped is set.
const OVERALL_TIMEOUT_MS = 120_000

/**
 * Owns the reconnect UX for an SSID change. Unlike the global ConnectionService
 * toast, an SSID change disconnects the WiFi client from the AP entirely — it
 * will NEVER reconnect on its own, the user must rejoin the new network. So this
 * is a persistent, dismissible:false dialog with an explicit instruction, not a
 * transient indicator. It polls until the router answers again (i.e. the user
 * has rejoined) and then reloads.
 */
@Component({
  template: `
    <div style="text-align: center; padding: 2rem;">
      @if (ssidConfirmed()) {
        <h3>{{ 'WiFi Restarted' | i18n }}</h3>
        <p>
          {{ 'Connect to' | i18n }}
          <strong>{{ ssid }}</strong>
          {{ 'to continue.' | i18n }}
        </p>
      } @else {
        <tui-loader size="l" />
        <p>{{ 'Restarting WiFi...' | i18n }}</p>
      }
    </div>
  `,
  imports: [TuiLoader, i18nPipe],
})
export class ReconnectDialog implements OnInit, OnDestroy {
  private readonly context =
    injectContext<
      TuiDialogContext<void, { ssid: string; done: Promise<boolean> }>
    >()
  private readonly api = inject(ApiService)
  private readonly connection = inject(ConnectionService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly i18n = inject(i18nPipe)
  private readonly isMock = inject(IS_MOCK)
  private intervalId?: ReturnType<typeof setInterval>
  private timeoutId?: ReturnType<typeof setTimeout>
  private networkDropped = false

  readonly ssidConfirmed = signal(false)

  protected get ssid(): string {
    return this.context.data.ssid
  }

  ngOnInit(): void {
    // This dialog owns the reconnect for the SSID change; mute the global
    // ConnectionService indicator so it doesn't also pop a toast.
    this.connection.suppress()
    this.context.data.done
      .then(saved => {
        // saved === false is the expected network-drop rejection; poll() drives
        // the rejoin and reload. A throw is a real backend failure — close and
        // surface it rather than spinning forever.
        if (saved) this.confirmSsid()
      })
      .catch(err => this.fail(err))
    this.poll()
    this.intervalId = setInterval(() => this.poll(), POLL_INTERVAL_MS)
    this.timeoutId = setTimeout(() => this.fail(), OVERALL_TIMEOUT_MS)
  }

  ngOnDestroy(): void {
    if (this.intervalId) clearInterval(this.intervalId)
    if (this.timeoutId) clearTimeout(this.timeoutId)
    // Defensive: every normal exit already resumes via cleanup(), but make sure
    // a destroy through any other path can't leave the global indicator muted.
    this.connection.resume()
  }

  private async poll(): Promise<void> {
    try {
      await this.api.systemInfo(PROBE_TIMEOUT_MS)
      // The router answered. Only finish once the SSID save landed (or we saw
      // the WiFi drop) — otherwise this is just the pre-restart connection still
      // alive, and we keep waiting for the disconnect.
      if (this.ssidConfirmed() || this.networkDropped) {
        this.cleanup()
        this.reloadOrClose()
      }
    } catch (e) {
      if (isNetworkError(e)) {
        // The WiFi client dropped off the old network — surface the
        // "reconnect to <ssid>" instruction and keep polling for the rejoin.
        this.networkDropped = true
        this.confirmSsid()
      } else {
        this.cleanup()
        this.reloadOrClose()
      }
    }
  }

  private reloadOrClose(): void {
    if (this.isMock) {
      this.context.completeWith()
    } else {
      window.location.reload()
    }
  }

  private cleanup(): void {
    if (this.intervalId) clearInterval(this.intervalId)
    if (this.timeoutId) clearTimeout(this.timeoutId)
    this.connection.resume()
  }

  /**
   * The save outcome is now known (it landed, or we observed the expected
   * drop). Show the "reconnect to <ssid>" instruction and disarm the overall
   * safety net — its only job is the sliver where neither flag ever gets set,
   * and a slow human rejoin past OVERALL_TIMEOUT_MS must not trip a false
   * "Failed to save" once we know the change actually went through.
   */
  private confirmSsid(): void {
    this.ssidConfirmed.set(true)
    if (this.timeoutId) clearTimeout(this.timeoutId)
    this.timeoutId = undefined
  }

  /**
   * Terminal failure path: the save errored (or the overall timeout fired).
   * Tear down (which resumes the global indicator), surface the error, and
   * close the dialog so the user is never trapped on the spinner.
   */
  private fail(error?: unknown): void {
    this.cleanup()
    this.alerts
      .open(
        (error as { message?: string })?.message ||
          this.i18n.transform('Failed to save'),
        { appearance: 'negative' },
      )
      .subscribe()
    this.context.completeWith()
  }
}
