import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiDataListWrapper, TuiSelect } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { StartOSDiskInfoWithId } from '../types'

interface Data {
  servers: StartOSDiskInfoWithId[]
}

@Component({
  imports: [
    FormsModule,
    TuiSelect,
    TuiDataListWrapper,
    i18nPipe,
    TuiHeader,
    TuiTitle,
  ],
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'Select Network Backup' | i18n }}</h2>
        <p>
          {{ 'Multiple backups found. Select which one to restore.' | i18n }}
        </p>
      </hgroup>
    </header>
    <tui-textfield [stringify]="stringify" [tuiTextfieldCleaner]="false">
      <label tuiLabel>{{ 'Backups' | i18n }}</label>
      <input tuiSelect [(ngModel)]="selectedServer" />
      <tui-data-list-wrapper
        *tuiDropdown
        [items]="context.data.servers"
        [itemContent]="serverContent"
      />
    </tui-textfield>

    <ng-template #serverContent let-server>
      <span tuiTitle>
        {{ server.hostname }}
        <span tuiSubtitle>
          {{ (server.scheduled ? 'Automatic backup' : 'Manual backup') | i18n }}
          · {{ server.version }}
        </span>
      </span>
    </ng-template>
  `,
  styles: `
    div {
      margin-block-end: 1rem;
    }
  `,
})
export class SelectNetworkBackupDialog {
  protected readonly context =
    injectContext<TuiDialogContext<StartOSDiskInfoWithId | null, Data>>()

  private _selectedServer: StartOSDiskInfoWithId | null = null

  get selectedServer(): StartOSDiskInfoWithId | null {
    return this._selectedServer
  }

  set selectedServer(value: StartOSDiskInfoWithId | null) {
    this._selectedServer = value

    if (value) {
      this.context.completeWith(value)
    }
  }

  readonly stringify = (server: StartOSDiskInfoWithId | null) =>
    server ? server.hostname : ''
}

export const SELECT_NETWORK_BACKUP = new PolymorpheusComponent(
  SelectNetworkBackupDialog,
)
