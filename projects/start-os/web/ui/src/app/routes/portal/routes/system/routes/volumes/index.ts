import { KeyValuePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import {
  ConvertBytesPipe,
  DialogService,
  ErrorService,
  HttpError,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiHint,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'

import { FormatDrive } from 'src/app/routes/portal/components/format-drive'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { Result } from 'src/app/services/api/api.types'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'
import { isVolumeInspectionStale } from 'src/app/utils/volume-inspection'
import { CREATE_VOLUME } from './create'
import { VOLUME_GRANTS } from './grants'

@Component({
  template: `
    <ng-container *title>
      <a tuiIconButton routerLink=".." iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Volumes' | i18n }}
    </ng-container>
    <header tuiHeader>
      <h3 tuiTitle>{{ 'Volumes' | i18n }}</h3>
      <aside tuiAccessories>
        <button formatDrive size="s"></button>
        <button tuiButton size="s" iconStart="@tui.plus" (click)="create()">
          {{ 'New' | i18n }}
        </button>
      </aside>
    </header>
    <table
      [appTable]="['Name', 'Source', 'Status', 'Free', 'Service access', null]"
    >
      @for (entry of volumes() | keyvalue: null; track entry.key) {
        @let id = entry.key;
        @let volume = entry.value;
        @let grants = volume.grants | keyvalue: null;
        <tr>
          <td>{{ volume.name }}</td>
          <td>
            @if (volume.source.kind === 'local') {
              {{ 'Local folder' | i18n }}
            } @else if (volume.source.kind === 'disk') {
              <span tuiTitle>
                {{ 'External drive' | i18n }}
                <span tuiSubtitle>{{ volume.source.logicalname }}</span>
                <span tuiSubtitle>{{ volume.source.path || '/' }}</span>
              </span>
            } @else {
              <span tuiTitle>
                {{ volume.source.hostname }}
                <span tuiSubtitle>{{ volume.source.path }}</span>
              </span>
            }
          </td>
          <td>
            <span tuiTitle>
              <span>
                <tui-icon
                  [icon]="
                    volume.source.kind === 'cifs'
                      ? '@tui.signal-high'
                      : volume.source.kind === 'disk'
                        ? '@tui.hard-drive'
                        : '@tui.folder'
                  "
                  [class.g-positive]="volume.availability === 'available'"
                  [class.g-negative]="volume.availability === 'unavailable'"
                  [style.font-size.rem]="1"
                />
                {{ statuses[volume.availability] | i18n }}
              </span>
              @if (volume.error; as error) {
                <span tuiSubtitle>{{ error }}</span>
              }
            </span>
          </td>
          <td>
            @if (volume.available !== null) {
              {{ volume.available | convertBytes }}
            } @else {
              &mdash;
            }
          </td>
          <td>{{ grants.length }}</td>
          <td>
            <button
              tuiIconButton
              tuiDropdown
              tuiDropdownAuto
              tuiDropdownAlign="end"
              size="s"
              appearance="flat-grayscale"
              iconStart="@tui.ellipsis-vertical"
            >
              {{ 'More' | i18n }}
              <tui-data-list *tuiDropdown="let close" (click)="close()">
                <button tuiOption iconStart="@tui.users" (click)="manage(id)">
                  {{ 'Service access' | i18n }}
                </button>
                <button
                  tuiOption
                  iconStart="@tui.pencil"
                  (click)="rename(id, volume)"
                >
                  {{ 'Rename' | i18n }}
                </button>
                <button
                  tuiOption
                  iconStart="@tui.trash-2"
                  (click)="remove(id, volume)"
                >
                  {{ 'Remove' | i18n }}
                </button>
              </tui-data-list>
            </button>
          </td>
        </tr>
      } @empty {
        <tr>
          <td colspan="6">{{ 'No volumes' | i18n }}</td>
        </tr>
      }
    </table>
    @let retainedItems = retained() | keyvalue: null;
    @if (retainedItems.length) {
      <section>
        <header tuiHeader>
          <h3 tuiTitle>{{ 'Pending local deletion' | i18n }}</h3>
        </header>
        <p>
          {{
            'Local data left after an unsuccessful deletion. Retry to delete it permanently.'
              | i18n
          }}
        </p>
        <table [appTable]="['Name', null]">
          @for (volume of retainedItems; track volume.key) {
            <tr>
              <td>{{ volume.value }}</td>
              <td>
                <button
                  tuiIconButton
                  size="s"
                  appearance="flat-grayscale"
                  iconStart="@tui.trash-2"
                  [tuiHint]="'Delete local data' | i18n"
                  (click)="deleteData(volume.key)"
                >
                  {{ 'Delete local data' | i18n }}
                </button>
              </td>
            </tr>
          }
        </table>
      </section>
    }
  `,
  imports: [
    RouterLink,
    i18nPipe,
    ConvertBytesPipe,
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiHint,
    TuiIcon,
    KeyValuePipe,
    TuiTitle,
    TuiHeader,
    TableComponent,
    FormatDrive,
    TitleDirective,
  ],
})
export default class Volumes {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly errors = inject(ErrorService)
  private readonly dialogs = inject(DialogService)

  protected readonly volumes = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('sharedVolumes'),
    { initialValue: {} as DataModel['sharedVolumes'] },
  )
  protected readonly retained = signal<Result<'volumes.list-retained'>>({})
  protected readonly statuses: Record<T.SharedVolumeAvailability, i18nKey> = {
    unknown: 'Unknown',
    available: 'Available',
    unavailable: 'Unavailable',
  }

  constructor() {
    this.refreshRetained()
    this.inspectStale()
  }

  private async refreshRetained() {
    try {
      this.retained.set(await this.api.rpc.volumes.listRetained({}))
    } catch (error) {
      this.errors.handleError(error as HttpError)
    }
  }

  protected create() {
    this.dialogs.openComponent(CREATE_VOLUME).subscribe()
  }

  protected manage(id: string) {
    this.dialogs.openComponent(VOLUME_GRANTS, { data: { id } }).subscribe()
  }

  private async inspectStale() {
    const now = Date.now()
    await Promise.all(
      Object.entries(this.volumes() || {})
        .filter(([, volume]) => isVolumeInspectionStale(volume.checkedAt, now))
        .map(async ([id]) => {
          try {
            await this.api.rpc.volumes.inspect({ id })
          } catch (error) {
            this.errors.handleError(error as HttpError)
          }
        }),
    )
  }

  protected async rename(id: string, volume: T.SharedVolume) {
    const name = await firstValueFrom(
      this.dialogs.openPrompt<string>({
        label: 'Rename',
        data: {
          message: 'Volume name',
          label: 'Name',
          initialValue: volume.name,
          buttonText: 'Save',
        },
      }),
      { defaultValue: null },
    )
    if (name === null) return
    return this.tasks.run(
      () => this.api.rpc.volumes.rename({ id, name }),
      'Saving',
    )
  }

  protected async remove(id: string, volume: T.SharedVolume) {
    const local = volume.source.kind === 'local'
    if (
      !(await firstValueFrom(
        this.dialogs.openConfirm({
          label: local ? 'Delete' : 'Remove',
          data: {
            content: local
              ? 'Delete this volume and all service access? Its local folder and all contents will be permanently deleted.'
              : 'Remove this volume and all service access? Data on the external drive or CIFS share will not be deleted.',
            yes: local ? 'Delete' : 'Remove',
            no: 'Cancel',
            appearance: local ? 'primary-destructive' : 'primary',
          },
        }),
        { defaultValue: false },
      ))
    )
      return
    await this.tasks.run(
      () => this.api.rpc.volumes.remove({ id }),
      local ? 'Deleting' : 'Removing',
    )
    await this.refreshRetained()
  }

  protected async deleteData(id: string) {
    if (
      !(await firstValueFrom(
        this.dialogs.openConfirm({
          label: 'Delete local data',
          data: {
            content:
              'Permanently delete this local volume folder and all its contents?',
            yes: 'Delete local data',
            no: 'Cancel',
            appearance: 'primary-destructive',
          },
        }),
        { defaultValue: false },
      ))
    )
      return
    if (
      await this.tasks.run(
        () => this.api.rpc.volumes.deleteData({ id }),
        'Deleting',
      )
    ) {
      await this.refreshRetained()
    }
  }
}
