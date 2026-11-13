import { DatePipe, KeyValuePipe } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import { ConvertBytesPipe, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiCell,
  TuiDialogContext,
  TuiHint,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBadge } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import {
  VOLUME_TYPES,
  volumeName,
  volumePath,
} from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { BackupService } from './backup.service'

export type PickerMode = 'create' | 'restore'

export interface Picked {
  targetId: string
  serverId: string
}

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        @if (context.data === 'create') {
          <h2 [id]="context.id">{{ 'Create backup' | i18n }}</h2>
          <p tuiSubtitle>{{ 'Select target' | i18n }}</p>
        } @else {
          <h2 [id]="context.id">{{ 'Restore from backup' | i18n }}</h2>
          <p tuiSubtitle>{{ 'Select backup' | i18n }}</p>
        }
      </hgroup>
    </header>
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
          <span tuiTitle>
            <strong>{{ name(target) }}</strong>
            <span tuiSubtitle>
              <tui-icon [icon]="types[target.source.kind].icon" />
              {{ path(target.source) }}
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
              }
            </span>
            @if (
              target.availability !== 'unavailable' && target.available !== null
            ) {
              <span tuiSubtitle>
                {{ 'Free' | i18n }}: {{ target.available | convertBytes }}
              </span>
            }
          </span>
          <tui-icon icon="@tui.chevron-right" />
        </button>
      } @empty {
        <app-placeholder icon="@tui.hard-drive">
          {{ 'No backup targets' | i18n }}
          <a
            tuiButton
            size="s"
            routerLink="/system/backup-targets"
            (click)="context.$implicit.complete()"
          >
            {{ 'Go to Backup Targets' | i18n }}
          </a>
        </app-placeholder>
      }
    } @else {
      @for (backup of backups(); track backup.targetId + backup.serverId) {
        <button
          tuiCell
          type="button"
          [disabled]="!backup.usable"
          (click)="context.completeWith(backup)"
        >
          <span tuiTitle>
            <strong>
              {{ backup.targetName }}
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
              <tui-icon [icon]="types[backup.source.kind].icon" />
              {{ path(backup.source) }}
            </span>
            <span tuiSubtitle class="server">
              <span>
                {{ backup.hostname }}.local · StartOS {{ backup.version }}
              </span>
              @if (backup.serverId === service.serverId()) {
                <span tuiBadge appearance="info" size="s">
                  {{ 'This server' | i18n }}
                </span>
              }
            </span>
            <span tuiSubtitle>{{ backup.timestamp | date: 'medium' }}</span>
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
        <app-placeholder icon="@tui.hard-drive">
          {{ 'No backups found on your backup targets.' | i18n }}
          <a
            tuiButton
            size="s"
            routerLink="/system/backup-targets"
            (click)="context.$implicit.complete()"
          >
            {{ 'Go to Backup Targets' | i18n }}
          </a>
        </app-placeholder>
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

    .server {
      flex-wrap: wrap;
    }

    strong,
    .server {
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
    RouterLink,
    TuiButton,
    PlaceholderComponent,
    DatePipe,
    KeyValuePipe,
    ConvertBytesPipe,
    TuiBadge,
    TuiCell,
    TuiIcon,
    TuiTitle,
    TuiHint,
    TuiHeader,
    i18nPipe,
  ],
})
class BackupPicker {
  protected readonly context =
    injectContext<TuiDialogContext<Picked, PickerMode>>()
  protected readonly service = inject(BackupService)
  protected readonly types = VOLUME_TYPES
  protected readonly name = volumeName
  protected readonly path = volumePath
  protected readonly oldFormat =
    'This backup uses an old format. It remains usable for restores. To enable stronger tamper detection and read-only snapshots, update it on the Backups page of the server this backup belongs to. Conversion may take a long time, and this backup is unavailable until it finishes.' as const

  protected readonly backups = computed(() =>
    Object.entries(this.service.targets())
      .flatMap(([targetId, target]) =>
        Object.entries(target.startOs).map(([serverId, backup]) => ({
          ...backup,
          targetId,
          serverId,
          targetName: volumeName(target),
          source: target.source,
          usable:
            target.availability !== 'unavailable' &&
            backup.formatStatus !== 'interrupted',
        })),
      )
      .sort((a, b) => (b.timestamp ?? '').localeCompare(a.timestamp ?? '')),
  )
}

export const BACKUP_PICKER = new PolymorpheusComponent(BackupPicker)
