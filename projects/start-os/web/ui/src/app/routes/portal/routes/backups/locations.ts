import { Component, inject, input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'

import { TitleDirective } from 'src/app/services/title.service'
import { BackupService } from '../system/routes/backups/backup.service'
import { BackupNetworkComponent } from '../system/routes/backups/network.component'
import { BackupPhysicalComponent } from '../system/routes/backups/physical.component'

@Component({
  selector: 'backup-locations',
  template: `
    @if (!embedded()) {
      <ng-container *title>
        <a
          tuiIconButton
          appearance="flat-grayscale"
          iconStart="@tui.arrow-left"
          routerLink="/system/backups"
        >
          {{ 'Back' | i18n }}
        </a>
        {{ 'Backup locations' | i18n }}
      </ng-container>

      <header class="heading">
        <span tuiTitle>
          <h2>{{ 'Backup locations' | i18n }}</h2>
          <span tuiSubtitle>
            {{
              'Use a physical drive or a shared folder on your local network.'
                | i18n
            }}
          </span>
        </span>
      </header>
    }

    <div tuiNotification appearance="info">
      {{
        'To add a physical location, connect a compatible drive to your Start9 Server, then refresh this page.'
          | i18n
      }}
    </div>

    <section networkFolders></section>
    <section physicalFolders></section>
  `,
  styles: `
    :host {
      display: grid;
      gap: 1rem;
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: none;
      margin-inline: auto;
    }

    section {
      inline-size: 100%;
      min-inline-size: 0;
    }

    h2 {
      margin: 0;
    }

    [tuiSubtitle] {
      display: block;
      margin-block-start: 0.25rem;
    }
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    RouterLink,
    TuiButton,
    TuiNotification,
    TuiTitle,
    TitleDirective,
    BackupNetworkComponent,
    BackupPhysicalComponent,
    i18nPipe,
  ],
})
export default class BackupLocations {
  readonly embedded = input(false)
  private readonly service = inject(BackupService)

  constructor() {
    void this.service.getBackupTargets()
  }
}
