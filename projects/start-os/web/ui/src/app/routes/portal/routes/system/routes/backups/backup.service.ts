import { computed, inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  DialogService,
  ErrorService,
  HttpError,
  RpcError,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T, Version } from '@start9labs/start-core'
import { TuiNotificationService } from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { isStale } from 'src/app/routes/portal/routes/system/routes/volumes/types'

@Injectable({ providedIn: 'root' })
export class BackupService {
  private readonly i18n = inject(i18nPipe)
  private readonly alerts = inject(TuiNotificationService)
  readonly scheduleTargets = computed(() =>
    Object.entries(this.targets()).map(([id, target]) => ({
      id,
      name: target.name,
    })),
  )
  targetName(id: string): string {
    return this.targets()[id]?.name || id
  }
  async getBackupTargets(): Promise<void> {
    await this.recheckStale()
  }
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly tasks = inject(TaskService)
  private readonly errors = inject(ErrorService)

  public readonly targets = toSignal(this.patch.watch$('backupTargets'), {
    initialValue: {} as DataModel['backupTargets'],
  })
  public readonly serverId = toSignal(this.patch.watch$('serverInfo', 'id'), {
    initialValue: '',
  })
  public readonly conversions = toSignal(
    this.patch.watch$('serverInfo', 'statusInfo', 'backupConversions'),
    { initialValue: {} as T.ServerStatus['backupConversions'] },
  )

  async recheckStale() {
    await Promise.all(
      Object.entries(this.targets())
        .filter(([, { checkedAt }]) => isStale(checkedAt))
        .map(([id]) =>
          this.api.rpc.backup.target
            .inspect({ id })
            .catch(error => this.errors.handleError(error as HttpError)),
        ),
    )
  }

  hasAnyBackup({ startOs }: Pick<T.BackupTarget, 'startOs'>): boolean {
    return Object.values(startOs).some(
      s => Version.parse(s.version).compare(Version.parse('0.3.6')) !== 'less',
    )
  }

  hasThisBackup(
    { startOs }: Pick<T.BackupTarget, 'startOs'>,
    id: string,
  ): boolean {
    const item = startOs[id]
    return (
      !!item &&
      Version.parse(item.version).compare(Version.parse('0.3.6')) !== 'less'
    )
  }

  hasCurrentBackup(target: Pick<T.BackupTarget, 'startOs'>): boolean {
    return this.hasThisBackup(target, this.serverId())
  }

  async prepareBackup(
    targetId: string,
    backups: Record<string, T.StartOsRecoveryInfo>,
  ): Promise<'ready' | 'conversion' | 'cancelled'> {
    const backup = backups[this.serverId()]
    const job = this.conversions()[targetId]?.[this.serverId()]
    if (job?.state === 'running' || backup?.formatStatus === 'interrupted')
      return 'conversion'
    if (backup?.formatStatus !== 'legacy') return 'ready'

    const update = await firstValueFrom(
      this.dialogs.openConfirm({
        label: 'Update backup format?',
        data: {
          content:
            'This server’s backup uses an old format. Update it for stronger tamper detection and read-only snapshots, or skip to create a backup using the existing format. Conversion may take a long time, and this backup is unavailable until it finishes.',
          yes: 'Update format',
          no: 'Skip',
        },
      }),
      { defaultValue: undefined },
    )
    if (update === undefined) return 'cancelled'
    if (!update) return 'ready'
    return (await this.convert(targetId, this.serverId()))
      ? 'conversion'
      : 'cancelled'
  }

  async convert(targetId: string, serverId: string): Promise<boolean> {
    const password = await firstValueFrom(
      this.dialogs.openPrompt<string>({
        label: 'Password required',
        data: {
          message:
            'Enter the password used to encrypt this backup. Conversion continues in the background after this dialog closes.',
          label: 'Master Password',
          placeholder: 'Enter master password',
          useMask: true,
        },
      }),
      { defaultValue: undefined },
    )
    if (password === undefined) return false
    return this.tasks.run(
      () =>
        this.api.rpc.backup.target.convert({ targetId, serverId, password }),
      'Starting backup conversion',
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
