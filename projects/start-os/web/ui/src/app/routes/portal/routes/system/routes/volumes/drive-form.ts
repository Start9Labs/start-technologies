import { Component, inject, viewChild } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  DialogService,
  HttpError,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiDialogContext } from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom, map, of, shareReplay, switchMap, tap } from 'rxjs'

import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import {
  Creator,
  Disk,
  diskSource,
  diskSpec,
  FORMAT,
  usableDrives,
} from './create'

type DriveValue = ReturnType<typeof diskSpec>['_TYPE']

function selectedDisk(drives: Disk[], value?: T.DeepPartial<DriveValue>) {
  const drive = value?.drive
  const disk = drives.find(disk => disk.logicalname === drive?.selection)
  const partition = drive?.value?.partition
  return disk &&
    (!partition ||
      partition === FORMAT ||
      disk.partitions.some(p => p.logicalname === partition))
    ? disk
    : undefined
}

@Component({
  template: `
    @if (spec(); as spec) {
      @for (current of [spec]; track current) {
        <app-form [spec]="current" [value]="value" [buttons]="buttons" />
      }
    } @else if (drives()?.length === 0) {
      <app-placeholder icon="@tui.hard-drive">
        {{ 'No drives detected' | i18n }}
      </app-placeholder>
    }
  `,
  imports: [FormComponent, PlaceholderComponent, i18nPipe],
})
export class DriveVolumeForm {
  private readonly api = inject(ApiService)
  private readonly create =
    injectContext<TuiDialogContext<void, { create: Creator }>>().data.create
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly form = viewChild(FormComponent)

  private readonly inventory = inject<PatchDB<DataModel>>(PatchDB)
    .watch$('disks')
    .pipe(
      map(usableDrives),
      tap(drives => {
        this.rememberValue()
        // Invalidate before asynchronous spec updates can coalesce inventory changes.
        if (this.value?.drive?.selection && !selectedDisk(drives, this.value)) {
          this.value = { ...this.value, drive: undefined }
          this.form()?.form.get('drive')?.reset()
        }
      }),
      shareReplay({ bufferSize: 1, refCount: true }),
    )

  protected value?: T.DeepPartial<DriveValue>
  protected readonly drives = toSignal(this.inventory, { initialValue: null })
  protected readonly spec = toSignal(
    this.inventory.pipe(
      switchMap(drives =>
        drives.length
          ? configBuilderToSpec(diskSpec(this.i18n, drives))
          : of(null),
      ),
      tap(() => this.rememberValue()),
    ),
    { initialValue: null },
  )
  protected readonly buttons = [
    {
      text: this.i18n.transform('Save'),
      handler: (value: DriveValue) => this.save(value),
    },
  ]

  private rememberValue() {
    this.value =
      (this.form()?.form.value as T.DeepPartial<DriveValue>) ?? this.value
  }

  private async save(value: DriveValue) {
    const { name, drive, path } = value
    const { partition } = drive.value
    const disk = selectedDisk(this.drives() ?? [], value)
    if (!disk) return false
    if (partition !== FORMAT)
      return this.tasks.run(
        () => this.create({ name, source: diskSource(partition, path) }),
        'Saving',
      )
    this.value = value
    if (!(await this.confirmErase(disk))) return false
    this.rememberValue()
    if (
      this.value?.drive?.selection !== drive.selection ||
      this.value?.drive?.value?.partition !== FORMAT ||
      !selectedDisk(this.drives() ?? [], this.value)
    )
      return false
    return this.tasks.run(async () => {
      const formatted = await this.api.rpc.disk.format({
        disk: disk.logicalname,
        label: name,
      })
      try {
        await this.create({ name, source: diskSource(formatted, path) })
      } catch (error) {
        throw {
          ...(error as HttpError),
          message: `${this.i18n.transform('The drive was formatted, but could not be added.')} ${(error as HttpError).message}`,
        }
      }
    }, 'Formatting drive')
  }

  private confirmErase(disk: Disk) {
    const backups = disk.partitions.some(
      p => p.legacyBackup || Object.keys(p.startOs).length,
    )
    return firstValueFrom(
      this.dialogs.openConfirm({
        label: 'Erase and format this drive',
        size: 's',
        data: {
          content: backups
            ? 'Everything on this drive will be permanently erased, including its StartOS backups.'
            : 'Everything on this drive will be permanently erased.',
          yes: 'Erase',
          no: 'Cancel',
          appearance: 'primary-destructive',
        },
      }),
      { defaultValue: false },
    )
  }
}
