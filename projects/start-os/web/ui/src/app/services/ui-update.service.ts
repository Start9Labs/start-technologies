import { inject, Service, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { SwUpdate } from '@angular/service-worker'
import { WA_SESSION_STORAGE, WA_WINDOW } from '@ng-web-apis/common'
import { Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { distinctUntilChanged, filter, map, merge, shareReplay } from 'rxjs'

import { ConfigService } from 'src/app/services/config.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

type UiUpdateState =
  | {
      status: 'idle' | 'checking' | 'updating' | 'ready' | 'resetting'
    }
  | { status: 'error'; error: string | null }

const RECOVERY_KEY = '_startos/ui-update-recovery'

@Service()
export class UiUpdateService {
  private readonly updates = inject(SwUpdate)
  private readonly win = inject(WA_WINDOW)
  private readonly storage = inject(WA_SESSION_STORAGE)
  private readonly config = inject(ConfigService)
  private readonly build = this.config.gitHash || this.config.version
  private readonly version = Version.parse(this.config.version)
  private readonly outdated$ = inject<PatchDB<DataModel>>(PatchDB)
    .watch$('serverInfo', 'version')
    .pipe(
      distinctUntilChanged(),
      map(version => this.version.compare(Version.parse(version)) !== 'equal'),
      shareReplay({ bufferSize: 1, refCount: true }),
    )
  private readonly current = signal<UiUpdateState>({ status: 'idle' })

  readonly state = this.current.asReadonly()
  readonly outdated = toSignal(this.outdated$, { initialValue: false })

  private readonly events = merge(
    this.updates.versionUpdates,
    this.updates.unrecoverable,
  )
    .pipe(takeUntilDestroyed())
    .subscribe(event => {
      if (this.state().status === 'resetting') return
      switch (event.type) {
        case 'VERSION_DETECTED':
          this.current.set({ status: 'updating' })
          break
        case 'VERSION_READY':
          this.current.set({ status: 'ready' })
          break
        case 'NO_NEW_VERSION_DETECTED':
          if (['idle', 'checking'].includes(this.state().status))
            this.current.set({ status: this.outdated() ? 'ready' : 'idle' })
          break
        case 'VERSION_INSTALLATION_FAILED':
          this.recover(event.error)
          break
        case 'UNRECOVERABLE_STATE':
          this.recover(event.reason)
          break
      }
    })

  private readonly versionChanges = this.outdated$
    .pipe(filter(Boolean), takeUntilDestroyed())
    .subscribe(() => {
      if (this.state().status === 'idle') void this.check()
    })

  async check(): Promise<void> {
    if (['checking', 'updating', 'resetting'].includes(this.state().status))
      return
    if (
      !this.updates.isEnabled ||
      this.win.navigator.serviceWorker.controller === null
    ) {
      this.current.set({ status: 'ready' })
      return
    }
    this.current.set({ status: 'checking' })
    try {
      const found = await this.updates.checkForUpdate()
      if (this.state().status === 'checking')
        this.current.set(
          found ? { status: 'ready' } : { status: 'error', error: null },
        )
    } catch (error) {
      if (this.state().status !== 'resetting') this.fail(error)
    }
  }

  reload(): void {
    if (this.state().status === 'ready') this.win.location.reload()
  }

  async resetCache(): Promise<void> {
    if (this.state().status === 'resetting') return
    this.current.set({ status: 'resetting' })
    try {
      const registration =
        await this.win.navigator.serviceWorker?.getRegistration()
      if (registration) {
        await registration.unregister()
        // Angular namespaces CacheStorage as ngsw:<scope pathname>:.
        const prefix = `ngsw:${new URL(registration.scope).pathname}:`
        const names = await this.win.caches.keys()
        await Promise.all(
          names
            .filter(name => name.startsWith(prefix))
            .map(name => this.win.caches.delete(name)),
        )
      }
      this.win.location.reload()
    } catch (error) {
      this.fail(error)
    }
  }

  private fail(error: unknown): void {
    this.current.set({ status: 'error', error: String(error) })
  }

  private recover(error: string): void {
    this.fail(error)
    if (!this.storage || this.storage.getItem(RECOVERY_KEY) === this.build)
      return
    this.storage.setItem(RECOVERY_KEY, this.build)
    this.win.location.reload()
  }
}
