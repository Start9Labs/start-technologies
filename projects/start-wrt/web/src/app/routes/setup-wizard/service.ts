import { DestroyRef, inject, Service, signal } from '@angular/core'
import { RpcError } from '@start9labs/shared'
import {
  ConnectionService,
  isNetworkError,
} from 'src/app/services/connection.service'
import {
  ApiService,
  type SetupFlashEvent,
  type SetupFlashReq,
} from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Service()
export class SetupFlash {
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly connection = inject(ConnectionService)
  private readonly lifetime = new AbortController()

  readonly running = signal(false)
  readonly status = signal(this.i18n.transform('Preparing...'))
  readonly step = signal(0)
  readonly totalSteps = signal(0)
  readonly copied = signal(0)
  readonly total = signal(0)
  readonly error = signal<string | null>(null)

  constructor() {
    inject(DestroyRef).onDestroy(() => this.lifetime.abort())
  }

  async run(params: SetupFlashReq): Promise<boolean> {
    const signal = this.lifetime.signal
    signal.throwIfAborted()
    this.running.set(true)
    this.error.set(null)
    this.step.set(0)
    this.totalSteps.set(0)
    this.copied.set(0)
    this.total.set(0)
    this.status.set(this.i18n.transform('Starting flash...'))
    try {
      const { items } = await this.api.setupFlash(params, { signal })
      signal.throwIfAborted()
      for await (const event of items) {
        signal.throwIfAborted()
        this.progress(event)
      }
      signal.throwIfAborted()
      return true
    } catch (error) {
      if (!signal.aborted) {
        if (isNetworkError(error)) this.connection.reportUnreachable()
        this.error.set(
          error instanceof Error || error instanceof RpcError
            ? error.message
            : this.i18n.transform('Flash failed'),
        )
      }
      return false
    } finally {
      this.running.set(false)
    }
  }

  private progress(event: SetupFlashEvent): void {
    if ('step' in event) this.step.set(event.step)
    if ('totalSteps' in event) this.totalSteps.set(event.totalSteps)
    switch (event.phase) {
      case 'copying':
        this.status.set(this.i18n.transform('Copying firmware...'))
        this.copied.set(event.copied)
        this.total.set(event.total)
        break
      case 'status':
        this.status.set(event.message)
        this.total.set(0)
        break
    }
  }
}
