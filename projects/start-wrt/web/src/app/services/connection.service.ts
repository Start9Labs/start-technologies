import { Component, inject, Injectable, signal } from '@angular/core'
import { TuiLoader, TuiNotificationService, TuiTitle } from '@taiga-ui/core'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { fromEvent, merge, Subscription } from 'rxjs'
import { filter, skip } from 'rxjs/operators'
import { ApiService } from 'src/app/services/api/api.service'
import { NetworkService } from 'src/app/services/network.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { IS_MOCK } from 'src/app/utils/workspace-config'

const POLL_INTERVAL_MS = 3000
// Per-probe timeout. Crucially this CANCELS the in-flight request (the RxJS
// `timeout` operator unsubscribes, which makes Angular abort the underlying
// XHR) so a wedged HTTP/2 connection — left dead after the router flushes
// conntrack mid-restart — gets torn down and the next probe opens a fresh one.
// A bare Promise.race would stop awaiting but leave the dead socket pooled, so
// every subsequent probe would multiplex onto it and stall for the browser's
// full TCP timeout.
const PROBE_TIMEOUT_MS = 5000
// How long a declared disruption's context (message / recovery intent) stays
// armed waiting for the connection to actually drop.
const CONTEXT_TTL_MS = 30_000
// Generous upper bound on how long we show "Reconnecting" before force-navigating
// to the new address regardless of the probe (reconnectAt only). Covers what the
// probe CAN'T detect: an untrusted self-signed HTTPS cert fails the background
// handshake silently, and a wedged connection may never recover in place. A real
// top-level navigation is the reliable recovery — it uses fresh connection state
// and surfaces the cert prompt, exactly like opening the address in a new window.
// Sized for a router network restart plus a Wi-Fi client DHCP renew.
const RECONNECT_FALLBACK_MS = 60_000

/** What to do once the router is reachable again. */
export type OnReconnect = 'none' | 'reload' | 'navigate'

export interface DisruptionOptions {
  /** Subtitle shown under the "Reconnecting" indicator. */
  message?: string
  onReconnect?: OnReconnect
  /**
   * Toast to show on recovery instead of the generic "Connection restored" —
   * lets a restart action's success ("WiFi settings saved") confirm AFTER the
   * router answers again rather than the moment it dropped.
   */
  successMessage?: string
  /**
   * Poll THIS absolute URL (cross-origin, no-cors) instead of systemInfo on the
   * current origin. Used when the current origin is gone for good (a bare-IP
   * address that the router is moving away from), so the systemInfo poll could
   * never recover.
   */
  probeUrl?: string
  /** Where to send the browser on recovery when onReconnect === 'navigate'. */
  navigateTo?: string
}

/**
 * The single sticky toast shown while the router is unreachable. Its subtitle
 * tracks {@link ConnectionService.message} so callers can tailor the copy (e.g.
 * the SSID to rejoin) without spawning a second toast.
 */
@Component({
  template: `
    <tui-loader size="m" [inheritColor]="true" />
    <div tuiTitle>
      {{ 'Reconnecting' | i18n }}
      <span tuiSubtitle>{{ message() }}</span>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      gap: 0.75rem;
      align-items: center;
    }
  `,
  imports: [i18nPipe, TuiLoader, TuiTitle],
})
class DisconnectedToast {
  protected readonly message = inject(ConnectionService).message
}

/**
 * Global owner of "router is unreachable" UX. Replaces the per-flow
 * RECONNECTING_DIALOG / ReconnectDialog modals and the old
 * NetworkRestartService.suppress() plumbing.
 *
 * Any network-level error (HttpError code 0) anywhere funnels here via
 * {@link reportUnreachable}; the first caller shows ONE sticky toast and starts
 * ONE cancelling poll loop, and concurrent callers (the ~15 background form
 * pollers) collapse into it. On recovery it dismisses the toast, confirms with
 * a single "Connection restored" toast, and runs the recovery intent.
 */
@Injectable({ providedIn: 'root' })
export class ConnectionService {
  private readonly alerts = inject(TuiNotificationService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly isMock = inject(IS_MOCK)

  /** True while the sticky "Reconnecting" indicator is shown. */
  readonly unreachable = signal(false)
  /** Subtitle for the indicator; consumed by {@link DisconnectedToast}. */
  readonly message = signal('')

  private toast?: Subscription
  private pollId?: ReturnType<typeof setInterval>
  private pending?: DisruptionOptions
  private pendingId?: ReturnType<typeof setTimeout>
  private onReconnect: OnReconnect = 'none'
  private successMessage = ''
  private probeUrl = ''
  private navigateTo = ''
  private fallbackId?: ReturnType<typeof setTimeout>
  private suppressed = false

  constructor() {
    // Re-probe the instant the browser reports the link is back (e.g. Wi-Fi
    // re-associates after the router reboots). A network transition flushes
    // stale sockets, so this probe lands on a fresh connection rather than the
    // wedged one the timer poll keeps reusing. skip(1) ignores the initial
    // "online" emission so we only react to a genuine offline→online flip.
    inject(NetworkService)
      .pipe(filter(Boolean), skip(1))
      .subscribe(() => this.unreachable() && this.poll())

    // Kick a fresh probe whenever the tab/link comes back to life. The
    // setInterval poll is throttled to a crawl (or frozen) while the tab is
    // backgrounded — e.g. the user opened the new address in another window —
    // and a Wi-Fi roam can restore connectivity without an offline→online flip,
    // so neither the timer nor the NetworkService hook above fires. Refocusing
    // the tab or the raw `online` event must re-probe so recovery isn't stuck
    // until the fallback. Guarded to only probe while unreachable and visible.
    merge(
      fromEvent(window, 'online'),
      fromEvent(window, 'focus'),
      fromEvent(document, 'visibilitychange'),
    ).subscribe(() => {
      if (this.unreachable() && document.visibilityState !== 'hidden') {
        this.poll()
      }
    })
  }

  /**
   * Declare that an in-flight action is about to restart the network. Pre-seeds
   * the indicator's (cosmetic) message so that whichever path next observes the
   * drop — the action's own error or a background poller — shows the right copy.
   * Shows nothing if connectivity survives the restart.
   *
   * Note: only the message is stashed, NOT the recovery intent. onReconnect is
   * honored solely from the reportUnreachable() call that observes the drop, so
   * a stale context can't trigger an unexpected reload (see reportUnreachable).
   * The stash lives at most CONTEXT_TTL_MS; worst case is a slightly stale
   * message on an unrelated blip in that window — harmless, clears on recover.
   */
  expectDisruption(opts: DisruptionOptions = {}): void {
    this.pending = { message: opts.message }
    clearTimeout(this.pendingId)
    this.pendingId = setTimeout(
      () => (this.pending = undefined),
      CONTEXT_TTL_MS,
    )
  }

  /**
   * Enter the unreachable state: show the sticky toast and poll until the router
   * responds. Idempotent — extra calls while already unreachable only refine the
   * message / recovery intent. No-op while {@link suppress}ed (a flow is owning
   * reconnection itself, e.g. an OS update, or the page is navigating away).
   */
  reportUnreachable(opts: DisruptionOptions = {}): void {
    if (this.suppressed) return

    // Honor the recovery intent ONLY from the call that actually observed the
    // drop — never inherited from a prior expectDisruption() context. Otherwise
    // a background-poller blip within CONTEXT_TTL_MS of e.g. a Wi-Fi SSID change
    // (whose caller armed 'reload') could reload the page out from under the
    // user. The cosmetic message may still fall back to the pending context.
    if (opts.onReconnect) this.onReconnect = opts.onReconnect
    // Set before the early-return below so a later/concurrent observer of the
    // same drop can still register the success copy / recovery target. If a
    // background poller armed the generic systemInfo poll first, setting
    // probeUrl here switches the running poll over to the destination probe on
    // its next tick.
    if (opts.successMessage) this.successMessage = opts.successMessage
    if (opts.probeUrl) this.probeUrl = opts.probeUrl
    if (opts.navigateTo) this.navigateTo = opts.navigateTo
    this.message.set(
      opts.message ||
        this.pending?.message ||
        this.i18n.transform('Trying to reach the router'),
    )

    if (this.unreachable()) return
    this.unreachable.set(true)

    this.toast = this.alerts
      .open(new PolymorpheusComponent(DisconnectedToast), {
        closable: false,
        appearance: 'negative',
        icon: '',
        // Persist until recover() unsubscribes; TuiNotificationService
        // otherwise auto-dismisses after its 3000ms default.
        autoClose: 0,
      })
      .subscribe()

    this.poll()
    this.pollId = setInterval(() => this.poll(), POLL_INTERVAL_MS)
  }

  /**
   * Suppress the indicator while a flow owns reconnection itself (OS update) or
   * the page is about to navigate to a new address (same-host IP change).
   */
  suppress(): void {
    this.suppressed = true
  }

  resume(): void {
    this.suppressed = false
  }

  /**
   * A subnet / router-IP change is moving the router to `target` (a same-scheme
   * absolute URL, e.g. `https://192.168.5.1` or `https://router.lan`). Show the
   * sticky "Reconnecting" toast, poll `target` until it answers, then navigate
   * there — instead of the systemInfo poll, because the current origin (a bare
   * IP being moved away from) may be gone for good. Resumes first so it takes
   * over from any background poller that already armed the generic poll.
   */
  reconnectAt(target: string, message: string): void {
    this.resume()
    this.reportUnreachable({
      message,
      probeUrl: target,
      navigateTo: target,
      onReconnect: 'navigate',
    })

    // Force the redirect after a generous wait even if the probe never confirms
    // reachability — the only recourse for an untrusted-HTTPS target (whose cert
    // defeats the background probe) or a wedged connection. The probe redirects
    // earlier whenever it can; a past-due timer fires on tab refocus, so a
    // backgrounded tab still recovers on return.
    clearTimeout(this.fallbackId)
    this.fallbackId = setTimeout(() => {
      if (this.unreachable()) this.recover()
    }, RECONNECT_FALLBACK_MS)
  }

  private async poll(): Promise<void> {
    // When a recovery target is armed, the current origin is gone — probe the
    // destination cross-origin instead of systemInfo on a dead origin.
    if (this.probeUrl) {
      if (await this.reachable(this.probeUrl)) this.recover()
      return
    }
    try {
      await this.api.systemInfo(PROBE_TIMEOUT_MS)
    } catch (e) {
      // Still unreachable only on a network-level failure; any RPC/HTTP
      // response (even an error) means the router is answering again.
      if (isNetworkError(e)) return
    }
    this.recover()
  }

  /**
   * Reachable = the router answered at all. A no-cors fetch resolves on ANY
   * response (opaque) and rejects only on a DNS/TLS/connection failure, which
   * is exactly the signal we want; we never read the body. The probe preserves
   * the page's scheme (no mixed-content block) and the cert is reissued for the
   * new IP and router.lan, so https validates once the router is back.
   *
   * Probes the tiny no-auth /static/root-ca.crt rather than the ~100KB SPA index
   * so a 3s poll stays cheap.
   *
   * Edge case: an untrusted self-signed HTTPS target (a client that hasn't
   * trusted this device's root CA) fails the background TLS handshake silently,
   * so this never resolves for that target — handled by reconnectAt's fallback
   * force-redirect (a top-level nav surfaces the cert prompt). http and
   * root-CA-trusting clients recover here directly.
   */
  private async reachable(url: string): Promise<boolean> {
    // Mock has no real router to probe; treat the destination as reachable so
    // the flow recovers (recover() skips the actual navigation under mock too).
    if (this.isMock) return true
    try {
      await fetch(`${url}/static/root-ca.crt?_probe=${Date.now()}`, {
        mode: 'no-cors',
        cache: 'no-store',
        // Bound each probe so an unroutable destination (the new subnet before
        // the client renews) aborts instead of piling up hung requests.
        signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
      })
      return true
    } catch {
      return false
    }
  }

  private recover(): void {
    if (!this.unreachable()) return

    clearInterval(this.pollId)
    this.pollId = undefined
    clearTimeout(this.fallbackId)
    this.fallbackId = undefined
    this.toast?.unsubscribe()
    this.toast = undefined
    this.unreachable.set(false)
    this.pending = undefined

    const onReconnect = this.onReconnect
    this.onReconnect = 'none'
    const successMessage = this.successMessage
    this.successMessage = ''
    const navigateTo = this.navigateTo
    this.navigateTo = ''
    this.probeUrl = ''

    if (onReconnect === 'navigate' && navigateTo && !this.isMock) {
      window.location.assign(navigateTo)
      return
    }

    if (onReconnect === 'reload' && !this.isMock) {
      window.location.reload()
      return
    }

    // Show the deferred action success ("WiFi settings saved") if one was armed,
    // otherwise the generic confirmation. Either way, only now that the router
    // has actually answered again.
    this.alerts
      .open(successMessage || this.i18n.transform('Connection restored'), {
        appearance: 'positive',
      })
      .subscribe()
  }
}

/** A network-level failure (no HTTP response): HttpError.status / code 0. */
export function isNetworkError(e: unknown): boolean {
  return (
    e != null && typeof e === 'object' && 'code' in e && (e as any).code === 0
  )
}
