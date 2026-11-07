import { Component, inject, signal, viewChild } from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import {
  DialogService,
  HttpError,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiDialogContext } from '@taiga-ui/core'
import { injectContext } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom, map, switchMap } from 'rxjs'

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

  protected readonly drives = signal<Disk[] | null>(null)
  protected readonly spec = signal<IST.InputSpec | null>(null)
  protected value?: DriveValue
  protected readonly buttons = [
    {
      text: this.i18n.transform('Save'),
      handler: (value: DriveValue) => this.save(value),
    },
  ]

  constructor() {
    inject<PatchDB<DataModel>>(PatchDB)
      .watch$('disks')
      .pipe(
        map(usableDrives),
        switchMap(async drives => ({
          drives,
          spec: drives.length
            ? await configBuilderToSpec(diskSpec(this.i18n, drives))
            : null,
        })),
        takeUntilDestroyed(),
      )
      .subscribe(({ drives, spec }) => {
        this.value = (this.form()?.form.value as DriveValue) ?? this.value
        this.drives.set(drives)
        this.spec.set(spec)
      })
  }

  private async save({ name, drive, path }: DriveValue) {
    const { partition } = drive.value
    if (partition !== FORMAT)
      return this.tasks.run(
        () => this.create({ name, source: diskSource(partition, path) }),
        'Saving',
      )
    const disk = this.drives()?.find(d => d.logicalname === drive.selection)
    if (!disk || !(await this.confirmErase(disk))) return false
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
