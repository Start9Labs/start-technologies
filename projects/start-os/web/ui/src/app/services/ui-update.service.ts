import { computed, inject, Service, signal } from '@angular/core'
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

  readonly state = this.current.asReadonly()
  readonly busy = computed(() =>
    ['resetting', 'reloading'].includes(this.state().status),
  )
  readonly phase = computed(() => {
    switch (this.state().status) {
      case 'ready':
      case 'reloading':
        return 'ready'
      case 'error':
        return 'error'
      default:
        return 'syncing'
    }
  })
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
            this.transition({
              status:
                this.outdated() || this.state().status === 'checking'
                  ? 'ready'
                  : 'idle',
            })
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
    if (this.state().status === 'ready') this.reloadPage()
  }

  async resetCache(): Promise<void> {
    if (this.busy()) return
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
      this.fail(error)
    }
  }

  private transition(state: UiUpdateState): void {
    if (this.state().status === 'reloading') return
    this.current.set(state)
  }

  private reloadPage(): void {
    this.current.set({ status: 'reloading' })
    this.win.location.reload()
  }

  private fail(error: unknown): void {
    this.transition({ status: 'error', error: String(error) })
  }
}
