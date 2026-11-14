import { DestroyRef, inject, Service, signal } from '@angular/core'
import {
  DialogService,
  ErrorService,
  formatProgress,
  TaskService,
} from '@start9labs/shared'
import { StartupApiService } from 'src/app/services/api/startup-api.service'

type UpdateObservation = Awaited<
  ReturnType<StartupApiService['streams']['diagnostic']['update']>
>

/** Successful termination confirms staging, not activation. Disposal cancels observation, not accepted work. */
@Service()
export class DiagnosticUpdate {
  private readonly api = inject(StartupApiService)
  private readonly tasks = inject(TaskService)
  private readonly errors = inject(ErrorService)
  private readonly dialog = inject(DialogService)
  private readonly lifetime = new AbortController()

  readonly progress = signal<ReturnType<typeof formatProgress> | null>(null)
  readonly target = signal<string | null>(null)
  readonly updated = signal(false)

  constructor() {
    inject(DestroyRef).onDestroy(() => this.lifetime.abort())
  }

  async update(): Promise<void> {
    const signal = this.lifetime.signal
    let operation: UpdateObservation | undefined
    await this.tasks.run(async () => {
      try {
        operation = await this.api.streams.diagnostic.update(
          { progress: true },
          { signal },
        )
      } catch (error) {
        if (!signal.aborted) throw error
      }
    }, 'Checking for updates')
    if (!operation) return

    try {
      signal.throwIfAborted()
      const target = operation.initial.target
      if (target) {
        this.target.set(target)
        this.progress.set({ total: 0, message: '' })
      }
      for await (const progress of operation.items) {
        signal.throwIfAborted()
        this.progress.set(formatProgress(progress))
      }
      signal.throwIfAborted()
      if (target) this.updated.set(true)
      else this.dialog.openAlert('Up to date!').subscribe()
    } catch (error) {
      if (!signal.aborted) this.errors.handleError(error)
    } finally {
      this.progress.set(null)
    }
  }
}
