import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { sameUrl } from '@start9labs/shared'
import { registryStatus } from '@start9labs/marketplace'
import { Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  BehaviorSubject,
  combineLatest,
  distinctUntilChanged,
  filter,
  firstValueFrom,
  map,
  shareReplay,
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { getServerInfo } from 'src/app/utils/get-server-info'
import { DataModel } from './patch-db/data-model'

@Injectable({
  providedIn: 'root',
})
export class OSService {
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private loadSequence = 0
  private readonly registry$ = this.patch
    .watch$('serverInfo', 'osRegistry')
    .pipe(distinctUntilChanged())

  private readonly catalog$ = combineLatest([
    this.patch.watch$('osUpdateCache'),
    this.registry$,
  ]).pipe(
    map(([cache, registry]) =>
      cache && sameUrl(cache.registry, registry) ? cache : null,
    ),
    distinctUntilChanged(),
    shareReplay({ bufferSize: 1, refCount: true }),
  )

  private readonly gitHash$ = new BehaviorSubject<string | null>(null)

  readonly registryStatus$ = this.catalog$.pipe(
    map(registryStatus),
    distinctUntilChanged(),
  )

  readonly updateCandidates$ = combineLatest([
    this.catalog$,
    this.gitHash$,
    this.patch.watch$('serverInfo', 'version'),
  ]).pipe(
    map(([cache, gitHash, installed]) => {
      const current = Version.parse(installed)
      return Object.entries(cache?.versions ?? {})
        .filter(([version, info]) => {
          const comparison = Version.parse(version).compare(current)
          return (
            comparison === 'greater' ||
            (comparison === 'equal' &&
              !!gitHash &&
              !!info.gitHash &&
              info.gitHash !== gitHash)
          )
        })
        .sort(([a], [b]) => Version.parse(b).compareForSort(Version.parse(a)))
        .map(([version, info]) => ({
          version,
          notes: info.releaseNotes,
          squashfs: info.squashfs,
        }))
    }),
    shareReplay({ bufferSize: 1, refCount: true }),
  )

  readonly updateCandidates = toSignal(this.updateCandidates$, {
    initialValue: [],
  })
  readonly updateAvailable$ = this.updateCandidates$.pipe(
    map(candidates => candidates.length > 0),
    distinctUntilChanged(),
  )

  private readonly statusInfo$ = this.patch
    .watch$('serverInfo', 'statusInfo')
    .pipe(shareReplay({ bufferSize: 1, refCount: true }))

  readonly updating$ = this.statusInfo$.pipe(
    map(status => status.updateProgress ?? false),
    distinctUntilChanged(),
  )

  readonly backingUp$ = this.statusInfo$.pipe(
    map(status => !!status.backupProgress),
    distinctUntilChanged(),
  )

  readonly updatingOrBackingUp$ = combineLatest([
    this.updating$,
    this.backingUp$,
  ]).pipe(map(([updating, backingUp]) => !!updating || backingUp))

  readonly showUpdate$ = combineLatest([
    this.updateAvailable$,
    this.updating$,
  ]).pipe(map(([available, updating]) => available && !updating))

  async publishesUpdates(registry: string): Promise<boolean> {
    const { id } = await getServerInfo(this.patch)
    const versions = await this.api.rpc.registry.os.version.get({
      registry,
      serverId: id,
    })

    return Object.keys(versions).length > 0
  }

  /** Registry changes supersede pending loads; catalog data comes from the matching cache. */
  async loadOS(registry?: string): Promise<void> {
    const sequence = ++this.loadSequence
    const target = registry ?? (await firstValueFrom(this.registry$))
    const gitHash = await this.api.rpc.gitInfo({})
    if (sequence !== this.loadSequence) return
    this.gitHash$.next(gitHash.trim())
    const [cache, selected] = await firstValueFrom(
      combineLatest([this.catalog$, this.registry$]).pipe(
        filter(
          ([cache, selected]) =>
            sequence !== this.loadSequence ||
            !sameUrl(selected, target) ||
            (!!cache && registryStatus(cache) !== 'connecting'),
        ),
      ),
    )
    if (sequence !== this.loadSequence || !sameUrl(selected, target)) return
    if (registryStatus(cache) === 'error')
      throw new Error('registry unavailable')
  }
}
