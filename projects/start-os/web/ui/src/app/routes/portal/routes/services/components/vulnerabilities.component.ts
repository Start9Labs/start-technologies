import { Component, computed, inject, input } from '@angular/core'
import { i18nPipe, LocalizePipe } from '@start9labs/shared'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { ControlsService } from 'src/app/services/controls.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { PrimaryStatus } from 'src/app/services/pkg-status-rendering.service'
import { getManifest } from 'src/app/utils/get-package-data'
import { RegistryVulnerability } from 'src/app/utils/vulnerabilities'

@Component({
  selector: 'service-vulnerabilities',
  template: `
    <header>{{ 'Vulnerabilities' | i18n }}</header>
    @for (vulnerability of vulnerabilities(); track vulnerability.key) {
      <div tuiNotification appearance="negative">
        <span tuiTitle>
          {{ vulnerability.id }}
          <span tuiSubtitle>
            {{ 'Vulnerable when' | i18n }}:
            {{ vulnerability.circumstances | localize }}
          </span>
          <span tuiSubtitle>
            {{ 'Impact' | i18n }}: {{ vulnerability.impact | localize }}
          </span>
        </span>
      </div>
    }
    @if (stopRecommended()) {
      <p>
        {{
          'Stopping this service is recommended until it is updated to a patched version.'
            | i18n
        }}
      </p>
      @if (status() === 'running') {
        <button
          tuiButton
          appearance="primary-destructive"
          size="s"
          (click)="controls.stop(manifest())"
        >
          {{ 'Stop' | i18n }}
        </button>
      }
    }
  `,
  styles: `
    :host {
      grid-column: span 10;
      display: flex;
      flex-direction: column;
      gap: 0.75rem;
    }

    button {
      align-self: start;
    }
  `,
  host: { class: 'g-card' },
  imports: [TuiButton, TuiNotification, TuiTitle, i18nPipe, LocalizePipe],
})
export class ServiceVulnerabilitiesComponent {
  protected readonly controls = inject(ControlsService)

  public readonly pkg = input.required<PackageDataEntry>()
  public readonly status = input<PrimaryStatus | string>()
  public readonly vulnerabilities = input.required<RegistryVulnerability[]>()

  protected readonly manifest = computed(() => getManifest(this.pkg()))
  protected readonly stopRecommended = computed(() =>
    this.vulnerabilities().some(({ stopRecommended }) => stopRecommended),
  )
}
