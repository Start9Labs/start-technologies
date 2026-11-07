import { DatePipe, KeyValuePipe } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { ConvertBytesPipe, i18nPipe } from '@start9labs/shared'
import {
  TuiCell,
  TuiDialogContext,
  TuiHint,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBadge } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import {
  VOLUME_TYPES,
  volumeLocation,
} from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { BackupService } from './backup.service'

export type PickerMode = 'create' | 'restore'

export interface Picked {
  targetId: string
  serverId: string
}

@Component({
  template: `
    @if (context.data === 'create') {
      @for (entry of service.targets() | keyvalue: null; track entry.key) {
        @let target = entry.value;
        <button
          tuiCell
          type="button"
          [disabled]="target.availability === 'unavailable'"
          (click)="
            context.completeWith({
              targetId: entry.key,
              serverId: service.serverId(),
            })
          "
        >
          <tui-icon [icon]="types[target.source.kind].icon" />
          <span tuiTitle>
            <strong>{{ target.name }}</strong>
            <span tuiSubtitle>
              {{ location(target.source) || ('This server' | i18n) }}
            </span>
            <span tuiSubtitle>
              @if (target.availability === 'unavailable') {
                <span class="g-negative">{{ 'Unavailable' | i18n }}</span>
              } @else {
                {{ 'Last backup' | i18n }}:
                @if (target.startOs[service.serverId()]; as backup) {
                  {{ backup.timestamp | date: 'medium' }}
                } @else {
                  {{ 'Never' | i18n }}
                }
                @if (target.available !== null) {
                  · {{ 'Free' | i18n }}: {{ target.available | convertBytes }}
                }
              }
            </span>
          </span>
          <tui-icon icon="@tui.chevron-right" />
        </button>
      } @empty {
        <p>{{ 'Add a backup target first.' | i18n }}</p>
      }
    } @else {
      @for (backup of backups(); track backup.targetId + backup.serverId) {
        <button
          tuiCell
          type="button"
          [disabled]="!backup.usable"
          (click)="context.completeWith(backup)"
        >
          <tui-icon icon="@tui.server" />
          <span tuiTitle>
            <strong>
              {{ backup.hostname }}.local
              @if (backup.serverId === service.serverId()) {
                <span tuiBadge appearance="info" size="s">
                  {{ 'This server' | i18n }}
                </span>
              }
              @if (backup.formatStatus === 'legacy') {
                <span
                  tuiBadge
                  appearance="warning"
                  size="s"
                  [tuiHint]="oldFormat | i18n"
                >
                  {{ 'Old format' | i18n }}
                </span>
              }
            </strong>
            <span tuiSubtitle>
              {{ backup.timestamp | date: 'medium' }} · StartOS
              {{ backup.version }}
            </span>
            <span tuiSubtitle>
              <tui-icon [icon]="types[backup.kind].icon" />
              {{ backup.targetName }}
            </span>
            @if (!backup.usable) {
              <span tuiSubtitle class="g-negative">
                {{
                  (backup.formatStatus === 'interrupted'
                    ? 'Conversion was interrupted. Resume it before using this backup.'
                    : 'Unavailable'
                  ) | i18n
                }}
              </span>
            }
          </span>
          <tui-icon icon="@tui.chevron-right" />
        </button>
      } @empty {
        <p>{{ 'No backups found on your backup targets.' | i18n }}</p>
      }
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
    }

    [tuiCell] {
      margin-inline: -1rem;
      padding-inline: 1rem;
      border-radius: var(--tui-radius-m);
      text-align: start;
    }

    strong {
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    [tuiSubtitle] tui-icon {
      font-size: 1rem;
      vertical-align: text-bottom;
    }

    p {
      margin: 0;
      color: var(--tui-text-secondary);
    }
  `,
  imports: [
    DatePipe,
    KeyValuePipe,
    ConvertBytesPipe,
    TuiBadge,
    TuiCell,
    TuiIcon,
    TuiTitle,
    TuiHint,
    i18nPipe,
  ],
})
class BackupPicker {
  protected readonly context =
    injectContext<TuiDialogContext<Picked, PickerMode>>()
  protected readonly service = inject(BackupService)
  protected readonly types = VOLUME_TYPES
  protected readonly location = volumeLocation
  protected readonly oldFormat =
    'This backup uses an old format. It remains usable for restores. To enable stronger tamper detection and read-only snapshots, update it on the Backups page of the server this backup belongs to. Conversion may take a long time, and this backup is unavailable until it finishes.' as const

  protected readonly backups = computed(() =>
    Object.entries(this.service.targets())
      .flatMap(([targetId, target]) =>
        Object.entries(target.startOs).map(([serverId, backup]) => ({
          ...backup,
          targetId,
          serverId,
          targetName: target.name,
          kind: target.source.kind,
          usable:
            target.availability !== 'unavailable' &&
            backup.formatStatus !== 'interrupted',
        })),
      )
      .sort((a, b) => (b.timestamp ?? '').localeCompare(a.timestamp ?? '')),
  )
}

export const BACKUP_PICKER = new PolymorpheusComponent(BackupPicker)
