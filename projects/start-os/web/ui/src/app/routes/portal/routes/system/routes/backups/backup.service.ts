import { inject, Injectable, signal } from '@angular/core'
import { ErrorService, getErrorMessage } from '@start9labs/shared'
import { T, Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'
import {
  CifsBackupTarget,
  DiskBackupTarget,
} from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

export interface MappedBackupTarget<T> {
  id: string
  hasAnyBackup: boolean
  hasCurrentBackup: boolean
  entry: T
}

@Injectable({
  providedIn: 'root',
})
export class BackupService {
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  private serverId = ''

  readonly cifs = signal<MappedBackupTarget<CifsBackupTarget>[]>([])
  readonly drives = signal<MappedBackupTarget<DiskBackupTarget>[]>([])
  readonly loading = signal(true)

  async getBackupTargets(): Promise<void> {
    this.loading.set(true)

    try {
      this.serverId = await firstValueFrom(
        this.patch.watch$('serverInfo', 'id'),
      )
      const targets = await this.api.getBackupTargets({})

      this.cifs.set(
        Object.entries(targets)
          .filter(([_, target]) => target.type === 'cifs')
          .map(([id, cifs]) => {
            return {
              id,
              hasAnyBackup: this.hasAnyBackup(cifs),
              hasCurrentBackup: this.hasCurrentBackup(cifs),
              entry: cifs as CifsBackupTarget,
            }
          }),
      )

      this.drives.set(
        Object.entries(targets)
          .filter(
            ([_, target]) => target.type === 'disk' && target.capacity > 0,
          )
          .map(([id, drive]) => {
            return {
              id,
              hasAnyBackup: this.hasAnyBackup(drive),
              hasCurrentBackup: this.hasCurrentBackup(drive),
              entry: drive as DiskBackupTarget,
            }
          }),
      )
    } catch (e: any) {
      this.errorService.handleError(getErrorMessage(e))
    } finally {
      this.loading.set(false)
    }
  }

  hasAnyBackup({ startOs }: T.BackupTarget): boolean {
    return Object.values(startOs).some(
      s => Version.parse(s.version).compare(Version.parse('0.3.6')) !== 'less',
    )
  }

  hasThisBackup({ startOs }: T.BackupTarget, id: string): boolean {
    const item = startOs[id]

    return (
      !!item &&
      Version.parse(item.version).compare(Version.parse('0.3.6')) !== 'less'
    )
  }

  // Whether *this* server has a current (V2) backup on the target — the signal
  // that decides if deleting the legacy backup needs an extra confirmation.
  hasCurrentBackup(target: T.BackupTarget): boolean {
    return this.hasThisBackup(target, this.serverId)
  }

  // Drop the now-deleted legacy (V1) backup from the cached target so the
  // warning + delete button disappear without re-listing every drive.
  clearLegacy(id: string): void {
    this.drives.update(drives =>
      drives.map(t =>
        t.id === id ? { ...t, entry: { ...t.entry, legacyBackup: false } } : t,
      ),
    )
    this.cifs.update(cifs =>
      cifs.map(t =>
        t.id === id ? { ...t, entry: { ...t.entry, legacyBackup: false } } : t,
      ),
    )
  }
}
