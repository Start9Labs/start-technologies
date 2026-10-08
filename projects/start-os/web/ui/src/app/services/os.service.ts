import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
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

  private readonly catalog$ = new BehaviorSubject<T.OsVersionInfoMap>({})

  readonly updateCandidates$ = combineLatest([
    this.catalog$,
    this.patch.watch$('serverInfo', 'version'),
  ]).pipe(
    map(([catalog, installed]) => {
      const current = Version.parse(installed)
      return Object.entries(catalog)
        .filter(
          ([version]) => Version.parse(version).compare(current) === 'greater',
        )
        .sort(([a], [b]) => Version.parse(b).compareForSort(Version.parse(a)))
        .map(([version, info]) => ({ version, notes: info.releaseNotes }))
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

  async loadOS(): Promise<void> {
    const { id } = await getServerInfo(this.patch)
    const { startosRegistry } = await firstValueFrom(this.patch.watch$('ui'))

    this.catalog$.next(
      await this.api.checkOSUpdate({
        registry: startosRegistry,
        serverId: id,
      }),
    )
  }
}
