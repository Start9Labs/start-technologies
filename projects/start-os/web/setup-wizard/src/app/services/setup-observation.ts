import { DestroyRef, inject, Service, signal } from '@angular/core'
import { Router } from '@angular/router'
import {
  formatProgress,
  getErrorMessage,
  isNetworkError,
  pauseFor,
} from '@start9labs/shared'
import { ApiService } from './api.service'

@Service()
export class SetupObservation {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly lifetime = new AbortController()

  readonly progress = signal({ total: 0, message: '' })
  readonly error = signal('')

  constructor() {
    inject(DestroyRef).onDestroy(() => this.lifetime.abort())
    void this.watch()
  }

  // Successful observation permits finalization; progress and transport loss do not.
  private async watch() {
    const { signal } = this.lifetime
    while (!signal.aborted) {
      try {
        const { initial, items } = await this.api.streams.setup.status(
          {},
          { signal },
        )
        signal.throwIfAborted()
        if (initial.status === 'running')
          this.progress.set(formatProgress(initial.progress))
        for await (const progress of items) {
          signal.throwIfAborted()
          this.progress.set(formatProgress(progress))
        }
        signal.throwIfAborted()
        await this.router.navigate([
          initial.status === 'running' || initial.status === 'complete'
            ? '/success'
            : '/language',
        ])
        return
      } catch (error) {
        if (signal.aborted) return
        if (!isNetworkError(error)) {
          this.error.set(getErrorMessage(error))
          return
        }
        try {
          await pauseFor(500, signal)
        } catch {
          return
        }
      }
    }
  }
}
