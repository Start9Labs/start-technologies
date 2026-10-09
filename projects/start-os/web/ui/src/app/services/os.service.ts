import { inject, Injectable } from '@angular/core'
import { leafProgress } from '@start9labs/shared'
import { T, Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  BehaviorSubject,
  combineLatest,
  distinctUntilChanged,
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

  osUpdate?: T.OsVersionInfoMap
  readonly updateAvailable$ = new BehaviorSubject<boolean>(false)

  private readonly statusInfo$ = this.patch
    .watch$('serverInfo', 'statusInfo')
    .pipe(shareReplay({ bufferSize: 1, refCount: true }))

  readonly updating$ = this.statusInfo$.pipe(
    map(status => status.updateProgress ?? false),
    distinctUntilChanged(),
  )

  readonly backingUp$ = this.statusInfo$.pipe(
    map(status => isBackupProgressActive(status.backupProgress)),
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

  async loadOS(): Promise<void> {
    const { version, id } = await getServerInfo(this.patch)
    const { startosRegistry } = await firstValueFrom(this.patch.watch$('ui'))

    this.osUpdate = await this.api.checkOSUpdate({
      registry: startosRegistry,
      serverId: id,
    })

    const latest = Object.entries(this.osUpdate).at(-1)?.[0]

    this.updateAvailable$.next(
      latest
        ? Version.parse(latest).compare(Version.parse(version)) === 'greater'
        : false,
    )
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
