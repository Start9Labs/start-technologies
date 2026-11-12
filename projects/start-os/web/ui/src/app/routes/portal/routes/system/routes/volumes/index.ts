import { KeyValuePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import {
  ConvertBytesPipe,
  DialogService,
  DocsLinkDirective,
  ErrorService,
  HttpError,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiHint, TuiIcon } from '@taiga-ui/core'
import { TuiTooltip } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { filter, firstValueFrom } from 'rxjs'

import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { Result } from 'src/app/services/api/api.types'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { TitleDirective } from 'src/app/services/title.service'
import { AddSourceService } from './add'
import { VolumeManage } from './manage'
import {
  isStale,
  VOLUME_STATUSES,
  VOLUME_TYPES,
  volumeName,
  volumePath,
} from './types'
import { VOLUME_SOURCES } from './source-picker'

import { DELETE_LOCAL } from './delete-local'
@Component({
  template: `
    <ng-container *title>
      <div>
        <a tuiIconButton routerLink=".." iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Shared Volumes' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/shared-volumes.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        {{ 'Shared Volumes' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/shared-volumes.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button
          tuiButton
          size="xs"
          iconStart="@tui.plus"
          class="actions"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </header>
      <table [appTable]="['Name', 'Type', 'Path', 'Status', 'Free', null]">
        @for (entry of volumes() | keyvalue: null; track entry.key) {
          @let id = entry.key;
          @let volume = entry.value;
          <tr>
            <td>{{ name(volume) }}</td>
            <td>
              <tui-icon
                [icon]="types[volume.source.kind].icon"
                [tuiHint]="types[volume.source.kind].label | i18n"
                [tuiHintShowDelay]="128"
              />
              <span class="mobile-path">{{ path(volume.source) }}</span>
            </td>
            <td class="path">{{ path(volume.source) }}</td>
            <td
              [class.g-positive]="volume.availability === 'available'"
              [class.g-negative]="volume.availability === 'unavailable'"
            >
              {{ statuses[volume.availability] | i18n }}
              @if (volume.error; as error) {
                <tui-icon [tuiTooltip]="error" />
              }
            </td>
            <td [attr.data-label]="'Free' | i18n">
              @if (volume.available !== null) {
                {{ volume.available | convertBytes }}
              } @else {
                &mdash;
              }
            </td>
            <td>
              <button
                tuiIconButton
                size="s"
                appearance="flat-grayscale"
                iconStart="@tui.settings"
                (click)="manage(id, volume)"
              >
                {{ 'Manage' | i18n }}
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td colspan="6">{{ 'No shared volumes' | i18n }}</td>
          </tr>
        }
      </table>
    </section>
    @let retainedItems = retained() | keyvalue: null;
    @if (retainedItems.length) {
      <section class="g-card">
        <header>{{ 'Pending local deletion' | i18n }}</header>
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
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    .actions {
      margin-inline-start: auto;
    }

    td:last-child:not([colspan]) {
      text-align: end;
    }

    td tui-icon {
      font-size: 1rem;
      vertical-align: text-bottom;
    }

    .mobile-path {
      display: none;
      margin-inline-start: 0.375rem;
    }

    :host-context(tui-root._mobile) {
      .g-card > header > [docsLink] {
        display: none;
      }

      tr {
        grid-template-columns: 1fr auto;
        row-gap: 0.25rem;
      }

      td {
        grid-column: 1;
      }

      td[data-label]::before {
        content: attr(data-label) ': ';
      }

      td:last-child:not([colspan]) {
        grid-column: 2;
        grid-row: 1 / span 4;
        align-self: center;
      }

      td.path {
        display: none;
      }

      .mobile-path {
        display: inline;
      }
    }
  `,
  imports: [
    RouterLink,
    DocsLinkDirective,
    i18nPipe,
    ConvertBytesPipe,
    TuiButton,
    TuiHint,
    TuiIcon,
    TuiTooltip,
    KeyValuePipe,
    TableComponent,
    TitleDirective,
  ],
})
export default class Volumes {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly errors = inject(ErrorService)
  private readonly dialogs = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly addSource = inject(AddSourceService)

  private readonly volumes$ =
    inject<PatchDB<DataModel>>(PatchDB).watch$('sharedVolumes')

  protected readonly volumes = toSignal(this.volumes$, { initialValue: {} })
  protected readonly retained = signal<Result<'volumes.list-retained'>>({})
  protected readonly types = VOLUME_TYPES
  protected readonly name = volumeName
  protected readonly path = volumePath
  protected readonly statuses = VOLUME_STATUSES

  constructor() {
    this.refreshRetained()
    this.recheckStale()
  }

  private async recheckStale() {
    const volumes = await firstValueFrom(this.volumes$)
    await Promise.all(
      Object.entries(volumes)
        .filter(([, { checkedAt }]) => isStale(checkedAt))
        .map(([id]) =>
          this.api.rpc.volumes
            .inspect({ id })
            .catch(error => this.errors.handleError(error as HttpError)),
        ),
    )
  }

  private async refreshRetained() {
    try {
      this.retained.set(await this.api.rpc.volumes.listRetained({}))
    } catch (error) {
      this.errors.handleError(error as HttpError)
    }
  }

  protected add() {
    this.addSource.open('Add shared volume', VOLUME_SOURCES, params =>
      this.api.rpc.volumes.create(params),
    )
  }

  protected manage(id: string, volume: T.SharedVolume) {
    this.formDialog
      .open<unknown, boolean>(VolumeManage, {
        label: 'Shared Volume',
        data: { id },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.remove(id, volume))
  }

  protected async remove(id: string, volume: T.SharedVolume) {
    const local = volume.source.kind === 'local'
    const confirmed = await firstValueFrom(
      local
        ? this.dialogs.openComponent<boolean>(DELETE_LOCAL, {
            label: 'Delete permanently',
            size: 's',
            data: {
              name: volume.name,
              warning:
                'Deleting this volume permanently erases its folder on this server and every file in it.',
            },
          })
        : this.dialogs.openConfirm({
            label: 'Remove',
            data: {
              content:
                'Remove this volume and all service access? Data on the external drive or CIFS share will not be deleted.',
              yes: 'Remove',
              no: 'Cancel',
            },
          }),
      { defaultValue: false },
    )
    if (!confirmed) return
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
