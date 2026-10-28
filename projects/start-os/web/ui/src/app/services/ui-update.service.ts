import { inject, Service, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { SwUpdate } from '@angular/service-worker'
import { WA_WINDOW } from '@ng-web-apis/common'
import { Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { distinctUntilChanged, filter, map, merge, shareReplay } from 'rxjs'

import { ConfigService } from 'src/app/services/config.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

type UiUpdateState =
  | {
      status:
        | 'idle'
        | 'checking'
        | 'updating'
        | 'ready'
        | 'resetting'
        | 'reloading'
    }
  | { status: 'error'; error: string | null }

@Service()
export class UiUpdateService {
  private readonly updates = inject(SwUpdate)
  private readonly win = inject(WA_WINDOW)
  private readonly version = Version.parse(inject(ConfigService).version)
  private readonly outdated$ = inject<PatchDB<DataModel>>(PatchDB)
    .watch$('serverInfo', 'version')
    .pipe(
      distinctUntilChanged(),
      map(version => this.version.compare(Version.parse(version)) !== 'equal'),
      shareReplay({ bufferSize: 1, refCount: true }),
    )
  private readonly current = signal<UiUpdateState>({ status: 'idle' })
  private readonly requested = signal(false)

  readonly state = this.current.asReadonly()
  readonly refreshing = this.requested.asReadonly()
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
          this.transition({ status: 'updating' })
          break
        case 'VERSION_READY':
          this.transition({ status: 'ready' })
          break
        case 'NO_NEW_VERSION_DETECTED':
          if (['idle', 'checking'].includes(this.state().status))
            this.transition({ status: this.outdated() ? 'ready' : 'idle' })
          break
        case 'VERSION_INSTALLATION_FAILED':
          this.fail(event.error)
          break
        case 'UNRECOVERABLE_STATE':
          this.fail(event.reason)
          break
      }
    })

  private readonly versionChanges = this.outdated$
    .pipe(filter(Boolean), takeUntilDestroyed())
    .subscribe(() => {
      if (this.state().status === 'idle') void this.check()
    })

  async check(): Promise<void> {
    if (
      ['checking', 'updating', 'resetting', 'reloading'].includes(
        this.state().status,
      )
    )
      return
    if (
      !this.updates.isEnabled ||
      this.win.navigator.serviceWorker.controller === null
    ) {
      this.transition({ status: 'ready' })
      return
    }
    this.transition({ status: 'checking' })
    try {
      const found = await this.updates.checkForUpdate()
      if (this.state().status === 'checking')
        this.transition(
          found ? { status: 'ready' } : { status: 'error', error: null },
        )
    } catch (error) {
      if (this.state().status !== 'resetting') this.fail(error)
    }
  }

  refresh(): void {
    if (['resetting', 'reloading'].includes(this.state().status)) return
    this.requested.set(true)
    if (!this.finishRefresh()) void this.check()
  }

  async resetCache(): Promise<void> {
    if (['resetting', 'reloading'].includes(this.state().status)) return
    this.requested.set(true)
    this.transition({ status: 'resetting' })
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
      this.reloadPage()
    } catch (error) {
      this.requested.set(false)
      this.fail(error)
    }
  }

  private transition(state: UiUpdateState): void {
    if (this.state().status === 'reloading') return
    this.current.set(state)
    this.finishRefresh()
  }

  private finishRefresh(): boolean {
    const { status } = this.state()
    if (this.refreshing() && (status === 'ready' || status === 'error')) {
      this.reloadPage()
      return true
    }
    return status === 'reloading'
  }

  private reloadPage(): void {
    this.current.set({ status: 'reloading' })
    this.win.location.reload()
  }

  private fail(error: unknown): void {
    this.transition({ status: 'error', error: String(error) })
  }
}
