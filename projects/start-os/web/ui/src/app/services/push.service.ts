import { Directive, inject, Service, signal } from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import { SwPush } from '@angular/service-worker'
import { WA_WINDOW } from '@ng-web-apis/common'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { EMPTY, catchError, firstValueFrom, from, mergeMap } from 'rxjs'

import { ApiService } from './api/embassy-api.service'

@Service({ autoProvided: false })
export class PushService {
  private readonly sw = inject(SwPush)
  private readonly win = inject(WA_WINDOW) as Window & typeof globalThis
  private readonly api = inject(ApiService)
  private readonly errors = inject(ErrorService)
  private readonly i18n = inject(i18nPipe)
  private pending: Promise<unknown> = Promise.resolve()
  private publicKey: string | null = null

  readonly available = this.sw.isEnabled && !!this.win.PushManager
  readonly enabled = signal(false)
  readonly ready = signal(false)

  private readonly rotations = this.sw.pushSubscriptionChanges
    .pipe(
      mergeMap(({ oldSubscription }) =>
        from(
          this.enqueue(async () => {
            const current = await this.synchronize()
            if (
              oldSubscription &&
              oldSubscription.endpoint !== current?.endpoint
            ) {
              await this.api.rpc.notification.push.unsubscribe({
                endpoint: oldSubscription.endpoint,
              })
            }
          }),
        ).pipe(
          catchError(error => {
            this.errors.handleError(error)
            return EMPTY
          }),
        ),
      ),
      takeUntilDestroyed(),
    )
    .subscribe()

  start() {
    if (!this.available) return
    return this.enqueue(() => this.synchronize())
      .catch(error => this.errors.handleError(error))
      .finally(() => this.ready.set(true))
  }

  enable() {
    const permission = this.win.Notification.requestPermission()
    return this.enqueue(async () => {
      if ((await permission) !== 'granted') {
        throw new Error(
          this.i18n.transform(
            'Notifications are blocked. Allow them in your browser settings.',
          ),
        )
      }
      const key = await this.key()
      const subscription =
        (await this.subscriptionForKey(key)) ??
        (await this.sw.requestSubscription({ serverPublicKey: key }))
      await this.register(subscription)
    })
  }

  disable() {
    return this.enqueue(async () => {
      const subscription = await firstValueFrom(this.sw.subscription)
      if (!subscription) {
        this.enabled.set(false)
        return
      }
      await this.sw.unsubscribe()
      this.enabled.set(false)
      await this.api.rpc.notification.push.unsubscribe({
        endpoint: subscription.endpoint,
      })
    })
  }

  private async synchronize() {
    const current = await firstValueFrom(this.sw.subscription)
    if (!current) {
      this.enabled.set(false)
      return null
    }
    const subscription = await this.subscriptionForKey(await this.key())
    if (subscription) await this.register(subscription)
    else this.enabled.set(false)
    return subscription
  }

  private async key() {
    return (this.publicKey ??= await this.api.rpc.notification.push.key({}))
  }

  private async subscriptionForKey(key: string) {
    const current = await firstValueFrom(this.sw.subscription)
    if (!current) return null
    const bytes = current.options.applicationServerKey
    const encoded = bytes
      ? this.win
          .btoa(String.fromCharCode(...new Uint8Array(bytes)))
          .replace(/\+/g, '-')
          .replace(/\//g, '_')
          .replace(/=+$/, '')
      : null
    if (encoded === key) return current
    await this.sw.unsubscribe()
    this.enabled.set(false)
    await this.api.rpc.notification.push.unsubscribe({
      endpoint: current.endpoint,
    })
    return this.sw.requestSubscription({ serverPublicKey: key })
  }

  private async register(subscription: PushSubscription) {
    const keys = subscription.toJSON().keys!
    await this.api.rpc.notification.push
      .subscribe({
        endpoint: subscription.endpoint,
        keys: { p256dh: keys['p256dh']!, auth: keys['auth']! },
      })
      .catch(error => {
        this.enabled.set(false)
        throw error
      })
    this.enabled.set(true)
  }

  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.pending.then(operation)
    this.pending = result.catch(() => {})
    return result
  }
}

@Directive()
export class BrowserPushSync {
  constructor() {
    inject(PushService).start()
  }
}
