import { KeyValuePipe } from '@angular/common'
import { Component, computed, inject, input, Pipe } from '@angular/core'
import {
  ConvertBytesPipe,
  DialogService,
  i18nPipe,
  LeafProgressPipe,
} from '@start9labs/shared'
import {
  TUI_BREAKPOINT,
  TuiButton,
  tuiButtonOptionsProvider,
  TuiDialogContext,
  tuiIconStart,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { ServiceProgressPhaseComponent } from 'src/app/routes/portal/routes/services/components/progress-phase.component'
import { BackupService } from './backup.service'
import { conversionLabel } from './conversion-state'

interface ConversionContext {
  targetId: string
}

@Pipe({ name: 'backupConversionLabel' })
export class ConversionLabel {
  transform = conversionLabel
}

@Component({
  selector: 'button[backupConversion]',
  template: `
    @if (breakpoint() !== 'mobile') {
      {{ localized() }}
    }
  `,
  hostDirectives: [TuiButton],
  host: {
    tuiButton: '',
    type: 'button',
    '(click.stop)': 'open()',
    '(keydown)': '$event.stopPropagation()',
    '[attr.aria-label]': 'localized()',
  },
  providers: [tuiButtonOptionsProvider({ appearance: 'warning', size: 'xs' })],
})
export class BackupConversion {
  private readonly dialogs = inject(DialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly icon = tuiIconStart(computed(() => '@tui.shield-alert'))

  protected readonly breakpoint = inject(TUI_BREAKPOINT)
  protected readonly localized = computed(() =>
    this.i18n.transform(this.label()),
  )

  public readonly targetId = input.required<string>()
  public readonly label =
    input.required<NonNullable<ReturnType<typeof conversionLabel>>>()

  protected open(): void {
    this.dialogs
      .openComponent(CONVERSIONS, {
        size: 'l',
        data: { targetId: this.targetId() },
      })
      .subscribe()
  }
}

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Backup format' | i18n }}</h2>
    </header>
    <p>
      {{
        'Old-format backups remain usable for backups and restores until you choose conversion.'
          | i18n
      }}
    </p>
    @for (backup of backups() | keyvalue; track backup.key) {
      @let job = service.conversions()[context.data.targetId]?.[backup.key];
      @if (
        backup.value.formatStatus === 'legacy' ||
        backup.value.formatStatus === 'interrupted' ||
        job
      ) {
        <section tuiCardLarge appearance="floating">
          <header tuiHeader>
            <h3 tuiTitle>
              {{ backup.value.hostname }}
              <span tuiSubtitle>{{ backup.key }}</span>
            </h3>
          </header>
          @if (job?.state === 'running') {
            <p>{{ 'Conversion in progress' | i18n }}</p>
          } @else if (job?.state === 'complete') {
            <div tuiNotification appearance="positive">
              {{ 'Conversion complete' | i18n }}
            </div>
          } @else if (backup.value.formatStatus === 'interrupted') {
            <div tuiNotification appearance="warning">
              {{
                'Conversion was interrupted. Resume it before using this backup.'
                  | i18n
              }}
            </div>
          } @else if (backup.value.formatStatus === 'legacy') {
            <div tuiNotification appearance="warning">
              {{
                'This backup uses the older integrity format. Conversion enables stronger tamper detection and read-only snapshots.'
                  | i18n
              }}
            </div>
          }
          @if (job) {
            @for (phase of job.progress.phases; track phase.name) {
              <service-progress-phase
                [name]="phase.name"
                [progress]="phase.progress"
              />
              @let leaf = phase.progress | leafProgress;
              @if (leaf && leaf !== true) {
                @if (leaf.units === 'bytes') {
                  <p>
                    {{ leaf.done | convertBytes }}
                    @if (leaf.total !== null) {
                      / {{ leaf.total | convertBytes }}
                    }
                  </p>
                } @else if (leaf.done > 0 || leaf.total !== null) {
                  <p>
                    {{ leaf.done }} / {{ leaf.total ?? '?' }}
                    {{ 'objects' | i18n }}
                  </p>
                }
              }
            }
            @if (job.error) {
              <div tuiNotification appearance="negative">{{ job.error }}</div>
            }
          }
          @if (job?.state !== 'running' && job?.state !== 'complete') {
            <button
              tuiButton
              size="s"
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
          }
        </section>
      }
    }
    <p>
      {{
        'You can close this dialog. Conversion continues in the background.'
          | i18n
      }}
    </p>
    <button
      tuiButton
      appearance="flat-grayscale"
      (click)="context.$implicit.complete()"
    >
      {{ 'Close' | i18n }}
    </button>
  `,
  styles: `
    section + section {
      margin-block-start: 1rem;
    }
  `,
  imports: [
    KeyValuePipe,
    TuiButton,
    TuiCardLarge,
    TuiHeader,
    TuiTitle,
    TuiNotification,
    ServiceProgressPhaseComponent,
    i18nPipe,
    LeafProgressPipe,
    ConvertBytesPipe,
  ],
})
class Conversions {
  protected readonly context =
    injectContext<TuiDialogContext<void, ConversionContext>>()
  protected readonly service = inject(BackupService)
  protected readonly backups = computed(
    () =>
      this.service.cifs().find(({ id }) => id === this.context.data.targetId)
        ?.entry.startOs ??
      this.service.drives().find(({ id }) => id === this.context.data.targetId)
        ?.entry.startOs ??
      {},
  )
}

const CONVERSIONS = new PolymorpheusComponent(Conversions)
