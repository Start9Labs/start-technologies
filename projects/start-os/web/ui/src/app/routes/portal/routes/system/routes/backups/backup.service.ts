import {
  computed,
  inject,
  Injectable,
  linkedSignal,
  signal,
} from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import {
  DialogService,
  ErrorService,
  getErrorMessage,
  TaskService,
} from '@start9labs/shared'
import { T, Version } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  defer,
  EMPTY,
  filter,
  finalize,
  firstValueFrom,
  from,
  pairwise,
  Subject,
  switchMap,
} from 'rxjs'

import {
  CifsBackupTarget,
  DiskBackupTarget,
} from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

export interface MappedBackupTarget<E> {
  id: string
  hasAnyBackup: boolean
  hasCurrentBackup: boolean
  entry: E
}

@Injectable({ providedIn: 'root' })
export class BackupService {
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly errorService = inject(ErrorService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly tasks = inject(TaskService)
  private readonly reload = new Subject<void>()
  private readonly targets = toSignal(
    this.reload.pipe(
      switchMap(() =>
        defer(() => {
          this.loading.set(true)
          return from(this.api.rpc.backup.target.list({})).pipe(
            catchError(error => {
              this.errorService.handleError(getErrorMessage(error))
              return EMPTY
            }),
            finalize(() => this.loading.set(false)),
          )
        }),
      ),
    ),
    { initialValue: {} as Record<string, T.BackupTarget> },
  )
  private readonly mapped = computed(() =>
    Object.entries(this.targets()).map(([id, entry]) => ({
      id,
      entry,
      hasAnyBackup: this.hasAnyBackup(entry),
      hasCurrentBackup: this.hasCurrentBackup(entry),
    })),
  )
  private readonly settled = this.patch
    .watch$('serverInfo', 'statusInfo', 'backupConversions')
    .pipe(
      pairwise(),
      filter(([previous, current]) =>
        Object.entries(current).some(([id, jobs]) =>
          Object.entries(jobs).some(
            ([server, job]) =>
              job.state !== 'running' &&
              previous[id]?.[server]?.state !== job.state,
          ),
        ),
      ),
      takeUntilDestroyed(),
    )
    .subscribe(() => this.getBackupTargets())

  public readonly serverId = toSignal(this.patch.watch$('serverInfo', 'id'), {
    initialValue: '',
  })
  public readonly conversions = toSignal(
    this.patch.watch$('serverInfo', 'statusInfo', 'backupConversions'),
    { initialValue: {} as T.ServerStatus['backupConversions'] },
  )
  public readonly cifs = linkedSignal(() =>
    this.mapped().filter(
      (target): target is MappedBackupTarget<CifsBackupTarget> =>
        target.entry.type === 'cifs',
    ),
  )
  public readonly drives = linkedSignal(() =>
    this.mapped().filter(
      (target): target is MappedBackupTarget<DiskBackupTarget> =>
        target.entry.type === 'disk' && target.entry.capacity > 0,
    ),
  )
  public readonly loading = signal(true)

  getBackupTargets(): void {
    this.reload.next()
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
    return this.hasThisBackup(target, this.serverId())
  }

  clearLegacy(id: string): void {
    const clear = <E extends T.BackupTarget>(
      targets: MappedBackupTarget<E>[],
    ) =>
      targets.map(target =>
        target.id === id
          ? { ...target, entry: { ...target.entry, legacyBackup: false } }
          : target,
      )
    this.drives.update(clear)
    this.cifs.update(clear)
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
    const confirmed = await firstValueFrom(
      this.dialogs.openConfirm({
        label: 'Convert backup format?',
        data: {
          content:
            'Conversion rewrites this backup and may take a long time. Keep the target connected. This backup is unavailable until conversion finishes, and older StartOS releases cannot read it afterward. An interrupted conversion can be resumed.',
          yes: 'Continue',
          no: 'Cancel',
        },
      }),
      { defaultValue: false },
    )
    if (!confirmed) return false
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
}
