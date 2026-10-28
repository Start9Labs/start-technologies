import {
  ChangeDetectionStrategy,
  Component,
  inject,
  input,
} from '@angular/core'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { ServiceProgressPhaseComponent } from './progress-phase.component'

@Component({
  selector: 'service-install-progress',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <header>
      {{ 'Install Progress' | i18n }}
      <button
        tuiButton
        size="xs"
        appearance="primary-destructive"
        [style.margin-inline-start]="'auto'"
        (click)="cancel()"
      >
        {{ 'Cancel' | i18n }}
      </button>
    </header>

    @for (
      phase of pkg().stateInfo.installingInfo?.progress?.phases;
      track phase.name
    ) {
      <service-progress-phase [name]="phase.name" [progress]="phase.progress" />
    }
  `,
  styles: `
    :host {
      grid-column: span 6;
      color: var(--tui-text-secondary);
    }
  `,
  host: { class: 'g-card' },
  imports: [ServiceProgressPhaseComponent, i18nPipe, TuiButton],
})
export class ServiceInstallProgressComponent {
  readonly pkg = input.required<PackageDataEntry>()

  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  async cancel() {
    this.tasks.run(
      async () =>
        await this.api.rpc.package.cancelInstall({
          id: getManifest(this.pkg()).id,
        }),
    )
  }
}
