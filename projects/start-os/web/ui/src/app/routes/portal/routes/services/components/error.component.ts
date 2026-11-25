import { Component, inject, Input } from '@angular/core'
import {
  DialogService,
  escapeHtml,
  i18nKey,
  i18nPipe,
} from '@start9labs/shared'
import { TuiButton, TuiIcon } from '@taiga-ui/core'
import { TuiTooltip } from '@taiga-ui/kit'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { StandardActionsService } from 'src/app/services/standard-actions.service'
import { getManifest } from 'src/app/utils/get-package-data'

@Component({
  selector: 'service-error',
  template: `
    <header>{{ 'Service Launch Error' | i18n }}</header>
    <p class="error-message">{{ error?.details }}</p>
    <h4>
      {{ 'Actions' | i18n }}
      <tui-icon [tuiTooltip]="hint" />
    </h4>
    <ng-template #hint>
      <p>
        {{
          '"Rebuild container" is a harmless action that and only takes a few seconds to complete. It will likely resolve this issue.'
            | i18n
        }}
      </p>
      <p>
        {{
          '"Soft uninstall" will remove the service from StartOS but preserve its data.'
            | i18n
        }}
      </p>
      <p>
        {{
          '"Hard uninstall" is a dangerous action that will remove the service from StartOS and wipe all its data.'
            | i18n
        }}
      </p>
    </ng-template>
    <p style="display: flex; flex-wrap: wrap; gap: 1rem">
      <button tuiButton (click)="rebuild()">
        {{ 'Rebuild container' | i18n }}
      </button>
      <button tuiButton appearance="warning" (click)="uninstall()">
        {{ 'Soft uninstall' | i18n }}
      </button>
      <button tuiButton appearance="negative" (click)="uninstall(false)">
        {{ 'Hard uninstall' | i18n }}
      </button>
      @if (overflow) {
        <button tuiButton appearance="secondary-grayscale" (click)="show()">
          {{ 'View full message' | i18n }}
        </button>
      }
    </p>
  `,
  styles: `
    :host {
      grid-column: span 5;
    }

    header {
      --tui-background-neutral-1: var(--tui-status-negative-pale);
    }

    .error-message {
      font-size: 1.5rem;
      color: var(--tui-status-negative);
      margin-bottom: 0;
    }

    h4 {
      display: flex;
      font: var(--tui-typography-body-m);
      font-weight: bold;
      color: var(--tui-text-secondary);
      text-transform: uppercase;
    }
  `,
  host: { class: 'g-card' },
  imports: [TuiButton, TuiIcon, TuiTooltip, i18nPipe],
})
export class ServiceErrorComponent {
  private readonly dialog = inject(DialogService)
  private readonly service = inject(StandardActionsService)

  @Input({ required: true })
  pkg!: PackageDataEntry

  overflow = false

  get error() {
    return this.pkg.statusInfo.error
  }

  rebuild() {
    this.service.rebuild(getManifest(this.pkg).id)
  }

  uninstall(soft = true) {
    this.service.uninstall(getManifest(this.pkg), { force: true, soft })
  }

  show() {
    this.dialog
      .openAlert(escapeHtml(this.error?.details || '') as i18nKey, {
        label: 'Service error',
      })
      .subscribe()
  }
}
