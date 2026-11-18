import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { leafProgress, sameUrl } from '@start9labs/shared'
import { registryStatus } from '@start9labs/marketplace'
import { T, Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
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

  readonly registryStatus$ = this.catalog$.pipe(
    map(registryStatus),
    distinctUntilChanged(),
  )

  readonly updateCandidates$ = combineLatest([
    this.catalog$,
    this.patch.watch$('serverInfo', 'gitHash'),
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
  private readonly statusInfo$ = this.patch
    .watch$('serverInfo', 'statusInfo')
    .pipe(shareReplay({ bufferSize: 1, refCount: true }))

  readonly updating$ = this.statusInfo$.pipe(
    map(status => status.updateProgress ?? false),
    distinctUntilChanged(),
  )

  readonly updated$ = this.statusInfo$.pipe(
    map(status => status.restart === 'update'),
    distinctUntilChanged(),
  )

  readonly backingUp$ = this.statusInfo$.pipe(
    map(status => isBackupProgressActive(status.backupProgress)),
    distinctUntilChanged(),
  )

  readonly updateDisabled$ = combineLatest([
    this.updating$,
    this.updated$,
    this.backingUp$,
  ]).pipe(
    map(([updating, updated, backingUp]) => !!updating || updated || backingUp),
  )

  readonly showUpdate$ = combineLatest([
    this.updateCandidates$,
    this.updating$,
    this.updated$,
  ]).pipe(
    map(
      ([candidates, updating, updated]) =>
        !!candidates.length && !updating && !updated,
    ),
    distinctUntilChanged(),
  )

  async publishesUpdates(registry: string): Promise<boolean> {
    const { id } = await getServerInfo(this.patch)
    const versions = await this.api.rpc.registry.os.version.get({
      registry,
      serverId: id,
    })

    return Object.keys(versions).length > 0
  }

  /** Settles once the selected registry's cache is published; a registry switch releases it. */
  async checkForUpdates(): Promise<void> {
    const target = await firstValueFrom(this.registry$)
    const [cache, selected] = await firstValueFrom(
      combineLatest([this.catalog$, this.registry$]).pipe(
        filter(
          ([cache, selected]) =>
            !sameUrl(selected, target) ||
            (!!cache && registryStatus(cache) !== 'connecting'),
        ),
      ),
    )
    if (!sameUrl(selected, target)) return
    if (registryStatus(cache) === 'error')
      throw new Error('registry unavailable')
  }
}

export function isBackupProgressActive(
  progress: T.ServerStatus['backupProgress'],
): boolean {
  if (!progress) return false

  const overall = leafProgress(progress.overall)
  if (overall === true) return false
  if (overall && typeof overall === 'object') return true

  return progress.phases.some(phase => {
    const phaseProgress = leafProgress(phase.progress)
    return phaseProgress !== null && phaseProgress !== true
  })
}
