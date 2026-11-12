import { DatePipe, NgTemplateOutlet } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiDialogContext, TuiIcon, TuiTitle, TuiCell } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'

import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { BACKUP_LIST_STYLES } from 'src/app/routes/portal/routes/system/routes/backups/list.styles'
import {
  VOLUME_TYPES,
  volumeName,
  volumePath,
} from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { DataModel } from '../services/patch-db/data-model'
import { formatDuration } from '../utils/format-duration'
import { getManifest } from '../utils/get-package-data'

type Result = { label: i18nKey; icon: string; class: string }

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'Backup Report' | i18n }}</h2>
        <p tuiSubtitle>{{ data.createdAt | date: 'medium' }}</p>
      </hgroup>
    </header>
    <div tuiCell>
      <span tuiAvatar size="s" aria-hidden="true">
        <img alt="" src="assets/img/icon.png" />
      </span>
      <span tuiTitle>
        <b>{{ 'System data' | i18n }}</b>
        @if (data.content.server.error; as error) {
          <span tuiSubtitle class="g-negative">{{ error }}</span>
        }
      </span>
      <span class="result">
        <span [class]="system().class">
          <tui-icon [icon]="system().icon" />
          {{ system().label | i18n }}
        </span>
        <span class="duration">&mdash;</span>
      </span>
    </div>
    @if (volumes().length) {
      <div class="group">
        <h3>{{ 'Shared Volumes' | i18n }}</h3>
      </div>
      @for (item of volumes(); track item.id) {
        <div tuiCell>
          <span
            tuiAvatar="@tui.folder-symlink"
            size="s"
            aria-hidden="true"
          ></span>
          <span tuiTitle>
            <b>{{ item.title }}</b>
            @if (item.source; as source) {
              <span tuiSubtitle>
                <tui-icon [icon]="types[source.kind].icon" />
                {{ path(source) }}
              </span>
            }
            @if (item.result.error; as error) {
              <span tuiSubtitle class="g-negative">{{ error }}</span>
            }
          </span>
          <ng-container
            [ngTemplateOutlet]="outcome"
            [ngTemplateOutletContext]="{ $implicit: item.result }"
          />
        </div>
      }
    }
    @if (packages().length) {
      <div class="group">
        <h3>{{ 'Services' | i18n }}</h3>
      </div>
      @for (item of packages(); track item.id) {
        <div tuiCell>
          <span tuiAvatar size="s" aria-hidden="true">
            <img [pkgIcon]="item.id" />
          </span>
          <span tuiTitle>
            <b>{{ item.title }}</b>
            @if (item.result.error; as error) {
              <span tuiSubtitle class="g-negative">{{ error }}</span>
            }
          </span>
          <ng-container
            [ngTemplateOutlet]="outcome"
            [ngTemplateOutletContext]="{ $implicit: item.result }"
          />
        </div>
      }
    }
    <ng-template #outcome let-result>
      @let status = toResult(result.error);
      <span class="result">
        <span [class]="status.class">
          <tui-icon [icon]="status.icon" />
          {{ status.label | i18n }}
        </span>
        <span class="duration">{{ formatDuration(result.duration_ms) }}</span>
      </span>
    </ng-template>
  `,
  styles: [
    BACKUP_LIST_STYLES,
    `
      header {
        margin-block-end: 1rem;
      }

      .result {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        font: var(--tui-typography-body-s);
        white-space: nowrap;

        tui-icon {
          font-size: 1rem;
          vertical-align: text-bottom;
        }
      }

      .duration {
        color: var(--tui-text-secondary);
      }
    `,
  ],
  imports: [
    DatePipe,
    NgTemplateOutlet,
    TuiAvatar,
    TuiCell,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    PkgIconDirective,
    i18nPipe,
  ],
})
export class BackupsReportModal {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly context =
    injectContext<
      TuiDialogContext<void, { content: T.BackupReport; createdAt: string }>
    >()
  protected readonly data = this.context.data

  protected readonly types = VOLUME_TYPES
  protected readonly path = volumePath
  protected readonly formatDuration = formatDuration

  private readonly packageData = toSignal(this.patch.watch$('packageData'))
  private readonly sharedVolumes = toSignal(this.patch.watch$('sharedVolumes'))

  protected readonly packages = computed(() =>
    Object.entries(this.data.content.packages).map(([id, result]) => {
      const pkg = this.packageData()?.[id]
      return { id, result, title: pkg ? getManifest(pkg).title : id }
    }),
  )

  protected readonly volumes = computed(() =>
    Object.entries(this.data.content.sharedVolumes ?? {}).map(
      ([id, result]) => {
        const volume = this.sharedVolumes()?.[id]
        return {
          id,
          result,
          title: volume ? volumeName(volume) : result.name,
          source: volume?.source,
        }
      },
    ),
  )

  protected readonly system = computed((): Result => {
    const { attempted, error } = this.data.content.server
    if (!attempted)
      return {
        label: 'Not attempted',
        icon: '@tui.minus',
        class: 'g-secondary',
      }
    return this.toResult(error)
  })

  protected toResult(error: string | null): Result {
    return error
      ? { label: 'Failed', icon: '@tui.circle-x', class: 'g-negative' }
      : { label: 'Succeeded', icon: '@tui.circle-check', class: 'g-positive' }
  }
}

export const REPORT = new PolymorpheusComponent(BackupsReportModal)
