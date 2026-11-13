import { KeyValuePipe } from '@angular/common'
import { Component, computed, inject, Pipe } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { ServiceProgressPhaseComponent } from 'src/app/routes/portal/routes/services/components/progress-phase.component'
import { BackupService } from './backup.service'
import { conversionLabel, serverBackups } from './conversion-state'

interface ConversionContext {
  targetId: string
}

@Pipe({ name: 'backupConversionLabel' })
export class ConversionLabel {
  transform = conversionLabel
}

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Backup format' | i18n }}</h2>
    </header>
    @for (backup of backups() | keyvalue; track backup.key) {
      @let job = service.conversions()[context.data.targetId]?.[backup.key];
      @if (job?.state === 'running') {
        <p>
          {{
            'Converting. This target is unavailable until it finishes.' | i18n
          }}
        </p>
      } @else if (job?.state === 'complete') {
        <div tuiNotification appearance="positive">
          {{ 'Conversion complete' | i18n }}
        </div>
      } @else {
        @if (job?.error; as error) {
          <div tuiNotification appearance="negative">{{ error }}</div>
        }
        <p>
          {{
            (backup.value.formatStatus === 'interrupted'
              ? 'Conversion was interrupted. Resuming continues where it left off.'
              : job?.state === 'failed'
                ? 'The last attempt didn’t finish. Retrying starts it again.'
                : 'Converting rewrites this backup with stronger tamper detection and read-only snapshots. Your server keeps running, but this target is unavailable until it finishes.'
            ) | i18n
          }}
        </p>
        <div tuiNotification appearance="warning">
          {{ durations[kind() ?? 'disk'] | i18n }}
        </div>
      }
      @if (job) {
        <service-progress-phase
          [name]="'Overall progress'"
          [progress]="job.progress"
          [showCounts]="true"
        />
      }
      @if (job?.state !== 'running' && job?.state !== 'complete') {
        <div class="actions">
          <button
            tuiButton
            iconStart="@tui.shield-check"
            (click)="service.convert(context.data.targetId, backup.key)"
          >
            {{
              (backup.value.formatStatus === 'interrupted'
                ? 'Resume conversion'
                : job?.state === 'failed'
                  ? 'Retry conversion'
                  : 'Convert backup'
              ) | i18n
            }}
          </button>
        </div>
      }
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    p {
      margin: 0;
    }

    .actions {
      display: flex;
      justify-content: flex-end;
    }
  `,
  imports: [
    KeyValuePipe,
    TuiButton,
    TuiHeader,
    TuiTitle,
    TuiNotification,
    ServiceProgressPhaseComponent,
    i18nPipe,
  ],
})
class Conversions {
  protected readonly context =
    injectContext<TuiDialogContext<void, ConversionContext>>()
  protected readonly service = inject(BackupService)
  protected readonly durations: Record<T.SharedVolumeSource['kind'], i18nKey> =
    {
      local: 'Large backups can take hours.',
      disk: 'Large backups can take hours. Keep the drive connected until it finishes.',
      cifs: 'Large backups can take many hours over the network. Keep the share reachable until it finishes.',
    }
  protected readonly kind = computed(
    () => this.service.targets()[this.context.data.targetId]?.source.kind,
  )
  protected readonly backups = computed(() =>
    serverBackups(
      this.service.targets()[this.context.data.targetId]?.startOs ?? {},
      this.service.serverId(),
    ),
  )
}

export const CONVERSIONS = new PolymorpheusComponent(Conversions)
