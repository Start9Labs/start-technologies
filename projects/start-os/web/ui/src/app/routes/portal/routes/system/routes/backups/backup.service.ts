import { computed, inject, Injectable, signal } from '@angular/core'
import {
  DialogService,
  convertBytes,
  ErrorService,
  getErrorMessage,
  i18nPipe,
  RpcError,
} from '@start9labs/shared'
import { T, Version } from '@start9labs/start-core'
import { TuiNotificationService } from '@taiga-ui/core'
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

export function formatCifsLocation(target: CifsBackupTarget): string {
  const share = target.path.replace(/^\/+/, '')
  return share ? `${target.hostname}/${share}` : target.hostname
}

@Injectable({
  providedIn: 'root',
})
export class BackupService {
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)
  private readonly i18n = inject(i18nPipe)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly alerts = inject(TuiNotificationService)
  private readonly dialogs = inject(DialogService)

  private serverId = ''

  readonly cifs = signal<MappedBackupTarget<CifsBackupTarget>[]>([])
  readonly drives = signal<MappedBackupTarget<DiskBackupTarget>[]>([])
  readonly loading = signal(true)
  readonly locations = computed(() => [
    ...this.cifs().map(location => ({
      id: location.id,
      location,
      name: location.entry.path.split('/').pop() || location.entry.path,
      detail: formatCifsLocation(location.entry),
      icon: '@tui.network',
      available: location.entry.mountable,
      capacity: null as number | null,
      used: null as number | null,
    })),
    ...this.drives().map(location => ({
      id: location.id,
      location,
      name:
        [location.entry.vendor, location.entry.model]
          .filter(Boolean)
          .join(' ') || location.entry.logicalname,
      detail: `${location.entry.logicalname} · ${convertBytes(location.entry.capacity)}`,
      icon: '@tui.hard-drive',
      available: location.entry.capacity > 0,
      capacity: location.entry.capacity,
      used: location.entry.used,
    })),
  ])

  readonly targets = computed(() =>
    this.locations().map(location => ({
      id: location.id,
      name:
        location.location.entry.type === 'cifs'
          ? location.detail
          : location.name,
    })),
  )

  targetName(id: string): string {
    return this.targets().find(target => target.id === id)?.name || id
  }

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
    } catch (e) {
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

  hasCurrentBackup(target: T.BackupTarget): boolean {
    return this.hasThisBackup(target, this.serverId)
  }

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

  showQueuedNotification(job: T.BackupJob): void {
    if (!job.status.runRequested) return

    this.alerts
      .open(
        this.i18n.transform(
          'The first backup is queued and will start automatically when no backup or restore is in progress.',
        ),
        {
          appearance: 'info',
          label: this.i18n.transform('Automatic backup'),
        },
      )
      .subscribe()
  }

  async withOriginalPassword<Result>(
    action: (oldPassword?: string) => Promise<Result>,
  ): Promise<Result | null> {
    let oldPassword: string | undefined
    for (;;) {
      try {
        return await action(oldPassword)
      } catch (error) {
        if (!(error instanceof RpcError) || error.code !== 81) throw error
      }
      oldPassword = await firstValueFrom(
        this.dialogs.openPrompt<string>({
          label: 'Original password needed',
          data: {
            message:
              'This backup was created with a different password. Enter the original password that was used to encrypt this backup.',
            label: 'Password',
            placeholder: 'Enter original password',
            useMask: true,
            buttonText: 'Retry',
          },
        }),
        { defaultValue: '' },
      )
      if (!oldPassword) return null
    }
  }
}
