import { Component, computed, inject, input, output } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import {
  TuiAppearance,
  TuiButton,
  TuiCell,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'

import {
  CifsBackupTarget,
  DiskBackupTarget,
} from 'src/app/services/api/api.types'
import {
  BackupService,
  MappedBackupTarget,
} from '../system/routes/backups/backup.service'

type Location = MappedBackupTarget<CifsBackupTarget | DiskBackupTarget>

@Component({
  selector: 'backup-location-picker',
  template: `
    <div class="locations">
      @for (target of targets(); track target.location.id) {
        <button
          tuiCell
          tuiAppearance="outline-grayscale"
          class="location-option"
          type="button"
          [disabled]="!target.available"
          [class._selected]="selectedId() === target.location.id"
          [class._manual-or-restore]="mode() !== 'automatic'"
          (click)="selected.emit(target.location)"
        >
          <tui-icon [icon]="target.icon" />
          <span tuiTitle>
            <b>{{ target.name }}</b>
            <span tuiSubtitle>
              <span class="target-detail">{{ target.detail }}</span>
              @if (!target.available) {
                <span class="target-reason">— {{ target.reason | i18n }}</span>
              }
            </span>
          </span>
          @if (selectedId() === target.location.id) {
            <tui-icon icon="@tui.circle-check" />
          }
        </button>
      } @empty {
        <p>{{ 'No backup locations are available.' | i18n }}</p>
      }
    </div>
    <button
      tuiButton
      class="manage-location"
      type="button"
      appearance="primary"
      iconStart="@tui.plus"
      (click)="manage.emit()"
    >
      {{ 'Add or repair a location' | i18n }}
    </button>
  `,
  styles: `
    :host,
    .locations {
      display: grid;
      gap: 0.5rem;
      justify-items: center;
    }

    :host {
      align-items: center;
      inline-size: 100%;
      max-inline-size: 48rem;
      margin-inline: auto;
      box-sizing: border-box;
      container-type: inline-size;
    }

    .locations {
      inline-size: 100%;
      max-inline-size: none;
      margin-inline: 0;
      box-sizing: border-box;
    }

    .locations > [tuiCell] {
      margin-block: 0;
    }

    .manage-location {
      justify-content: flex-start;
      block-size: auto;
      padding-block: 0.75rem;
      white-space: normal;
      text-align: start;
    }

    [tuiCell] {
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: 100%;
      align-items: center;
      gap: 0.75rem;
      overflow: hidden;
      text-align: start;
      block-size: auto;
      min-block-size: 4rem;
      padding-block: 0.75rem;
      white-space: normal;
      box-sizing: border-box;
    }

    .location-option,
    .manage-location {
      inline-size: 100%;
      max-inline-size: 40rem;
      margin-block: 0;
      margin-inline: 0;
      justify-self: center;
      box-sizing: border-box;
    }

    [tuiTitle] {
      flex: 1;
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    [tuiSubtitle] {
      display: block;
      margin-block-start: 0.25rem;
    }

    ._selected {
      box-shadow: inset 0 0 0 2px var(--tui-border-focus);
    }

    p {
      color: var(--tui-text-secondary);
    }

    ._manual-or-restore > [tuiTitle] {
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(8rem, 45%);
      align-items: center;
      gap: 0.75rem;
      inline-size: 100%;
    }

    ._manual-or-restore.location-option {
      inline-size: 100%;
      box-sizing: border-box;
    }

    ._manual-or-restore > [tuiTitle] > b {
      grid-column: 1;
    }

    ._manual-or-restore > [tuiTitle] [tuiSubtitle] {
      grid-column: 2;
      min-inline-size: 0;
      margin-block-start: 0;
      overflow-wrap: anywhere;
      text-align: end;
    }

    @container (max-inline-size: 30rem) {
      [tuiCell] {
        padding-inline: 0.75rem;
      }

      .location-option,
      .manage-location {
        justify-self: center;
      }

      ._manual-or-restore > [tuiTitle] {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        column-gap: 0.5rem;
        row-gap: 0;
        min-inline-size: 0;
      }

      .location-option > [tuiTitle] > b {
        display: block;
        flex: 1 1 auto;
        min-inline-size: 0;
        max-inline-size: 100%;
        overflow-wrap: normal;
        white-space: normal;
        word-break: normal;
      }

      ._manual-or-restore > [tuiTitle] [tuiSubtitle] {
        display: flex;
        flex: 1 1 100%;
        flex-wrap: wrap;
        justify-content: flex-start;
        min-inline-size: 0;
        max-inline-size: 100%;
        margin-inline-start: 0;
        overflow-wrap: anywhere;
        text-align: start;
        white-space: normal;
        word-break: break-word;
      }

      ._manual-or-restore > [tuiTitle] .target-detail {
        min-inline-size: 0;
        max-inline-size: 100%;
        overflow-wrap: anywhere;
        white-space: normal;
        word-break: break-word;
      }

      ._manual-or-restore > [tuiTitle] .target-reason {
        overflow-wrap: normal;
        white-space: normal;
        word-break: normal;
      }
    }
  `,
  imports: [TuiAppearance, TuiButton, TuiCell, TuiIcon, TuiTitle, i18nPipe],
})
export class BackupLocationPicker {
  private readonly backupService = inject(BackupService)

  readonly mode = input.required<'automatic' | 'manual' | 'restore'>()
  readonly selectedId = input('')
  readonly selected = output<Location>()
  readonly manage = output<void>()

  protected readonly targets = computed(() =>
    this.backupService.locations().map(target => ({
      ...target,
      available:
        target.available &&
        (this.mode() !== 'restore' || target.location.hasAnyBackup),
      reason: target.available ? 'No backups found' : 'Unavailable',
    })),
  )
}
