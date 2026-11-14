import { DestroyRef, inject, Service, signal } from '@angular/core'
import { formatProgress, i18nPipe, pauseFor } from '@start9labs/shared'
import { StartupApiService } from 'src/app/services/api/startup-api.service'
import { StateService } from 'src/app/services/state.service'

@Service()
export class InitializingProgress {
  private readonly api = inject(StartupApiService)
  private readonly state = inject(StateService)
  private readonly lifetime = new AbortController()

  readonly progress = signal({
    total: 0,
    message: inject(i18nPipe).transform('waiting'),
  })

  constructor() {
    inject(DestroyRef).onDestroy(() => this.lifetime.abort())
    void this.watch()
  }

  // State probes establish readiness; progress and socket closure cannot.
  private async watch() {
    const { signal } = this.lifetime
    while (!signal.aborted) {
      try {
        const { initial, items } = await this.api.streams.init.subscribe(
          {},
          { signal },
        )
        signal.throwIfAborted()
        this.progress.set(formatProgress(initial.progress))
        for await (const progress of items) {
          signal.throwIfAborted()
          this.progress.set(formatProgress(progress))
        }
        if (!signal.aborted) this.state.retrigger(true, 250)
        return
      } catch {
        if (signal.aborted) return
        this.state.retrigger(true, 250)
        try {
          await pauseFor(500, signal)
        } catch {
          return
        }
      }
    }
  }
}
