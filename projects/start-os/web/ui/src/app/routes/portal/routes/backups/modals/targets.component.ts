import { Component, inject, OnInit, signal } from '@angular/core'
import {
  DocsLinkDirective,
  ErrorService,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiLink, TuiNotification } from '@taiga-ui/core'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { RR, UnknownDisk } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import { BackupsPhysicalComponent } from '../components/physical.component'
import { BackupsTargetsComponent } from '../components/targets.component'
import { BackupConfig } from '../types/backup-config'
import {
  cifsSpec,
  diskBackupTargetSpec,
  dropboxSpec,
  googleDriveSpec,
  remoteBackupTargetSpec,
} from '../types/target'

@Component({
  template: `
    <div tuiNotification>
      Backup targets are physical or virtual locations for storing encrypted
      backups. They can be physical drives plugged into your server, shared
      folders on your Local Area Network (LAN), or third party clouds such as
      Dropbox or Google Drive.
      <a tuiLink docsLink path="/start-os/backup-create.html">
        View instructions
      </a>
    </div>
    <h3 class="g-title">
      Unknown Physical Drives
      <button
        tuiButton
        size="s"
        iconStart="@tui.refresh-cw"
        (click)="refresh()"
      >
        Refresh
      </button>
    </h3>
    <table
      class="g-table"
      [backupsPhysical]="targets()?.unknownDisks || null"
      (add)="addPhysical($event)"
    ></table>
    <h3 class="g-title">
      Saved Targets
      <button tuiButton size="s" iconStart="@tui.plus" (click)="addRemote()">
        Add Target
      </button>
    </h3>
    <table
      class="g-table"
      [backupsTargets]="targets()?.saved || null"
      (delete)="onDelete($event)"
      (update)="onUpdate($event)"
    ></table>
  `,
  imports: [
    TuiNotification,
    TuiButton,
    BackupsPhysicalComponent,
    BackupsTargetsComponent,
    TuiLink,
    DocsLinkDirective,
  ],
})
export class BackupsTargetsModal implements OnInit {
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)
  private readonly formDialog = inject(FormDialogService)
  private readonly tasks = inject(TaskService)

  targets = signal<RR.GetBackupTargetsRes | null>(null)

  ngOnInit() {
    this.refresh()
  }

  async refresh() {
    this.targets.set(null)

    try {
      this.targets.set(await this.api.rpc.backup.target.list({}))
    } catch (e: any) {
      this.errorService.handleError(e)
      this.targets.set({ unknownDisks: [], saved: {} })
    }
  }

  async onDelete(id: string) {
    this.tasks.run(async () => {
      await this.api.rpc.backup.target.cifs.remove({ id })

      const saved = this.targets()?.saved || {}

      delete saved[id]

      this.setTargets(saved)
    }, 'Removing')
  }

  async onUpdate(id: string) {
    const value = this.targets()?.saved[id]

    if (!value) return

    this.formDialog.open(FormComponent, {
      label: 'Update Target',
      data: {
        value,
        spec: await this.getSpec(value),
        buttons: [
          {
            text: 'Save',
            handler: (
              response:
                | RR.UpdateCifsBackupTargetReq
                | RR.UpdateCloudBackupTargetReq
                | RR.UpdateDiskBackupTargetReq,
            ) => this.update(value.type, { ...response, id }),
          },
        ],
      },
    })
  }

  async addPhysical(disk: UnknownDisk) {
    this.formDialog.open(FormComponent, {
      label: 'New Physical Target',
      data: {
        spec: await configBuilderToSpec(diskBackupTargetSpec),
        value: { name: disk.label || disk.logicalname },
        buttons: [
          {
            text: 'Save',
            handler: (value: Omit<RR.AddDiskBackupTargetReq, 'logicalname'>) =>
              this.add(
                'disk',
                {
                  logicalname: disk.logicalname,
                  ...value,
                },
                response => {
                  const [id, entry] = Object.entries(response)[0]
                  const saved = this.targets()?.saved || {}

                  saved[id] = entry

                  this.setTargets(
                    saved,
                    this.targets()?.unknownDisks.filter(a => a !== disk),
                  )
                },
              ),
          },
        ],
      },
    })
  }

  async addRemote() {
    this.formDialog.open(FormComponent, {
      label: 'New Remote Target',
      data: {
        spec: await configBuilderToSpec(remoteBackupTargetSpec),
        buttons: [
          {
            text: 'Save',
            handler: ({ type }: BackupConfig) =>
              this.add(
                type.selection === 'cifs' ? 'cifs' : 'cloud',
                type.value,
              ),
          },
        ],
      },
    })
  }

  private async add(
    type: T.BackupTargetType,
    value:
      | RR.AddCifsBackupTargetReq
      | RR.AddCloudBackupTargetReq
      | RR.AddDiskBackupTargetReq,
    handler: (response: RR.AddBackupTargetRes) => void = () => {},
  ): Promise<boolean> {
    return this.tasks.run(
      async () =>
        handler(await this.api.rpc.backup.target.cifs.add(type, value)),
      'Saving target',
    )
  }

  private async update(
    type: T.BackupTargetType,
    value:
      | RR.UpdateCifsBackupTargetReq
      | RR.UpdateCloudBackupTargetReq
      | RR.UpdateDiskBackupTargetReq,
  ): Promise<boolean> {
    return this.tasks.run(
      async () => await this.api.rpc.backup.target.cifs.update(type, value),
      'Saving target',
    )
  }

  private setTargets(
    saved: Record<string, T.BackupTarget> = this.targets()?.saved || {},
    unknownDisks: UnknownDisk[] = this.targets()?.unknownDisks || [],
  ) {
    this.targets.set({ unknownDisks, saved })
  }

  private async getSpec(target: T.BackupTarget) {
    switch (target.type) {
      case 'cifs':
        return await configBuilderToSpec(cifsSpec)
      case 'cloud':
        return await configBuilderToSpec(
          target.provider === 'dropbox' ? dropboxSpec : googleDriveSpec,
        )
      case 'disk':
        return await configBuilderToSpec(diskBackupTargetSpec)
    }
  }
}

export const TARGETS = new PolymorpheusComponent(BackupsTargetsModal)
