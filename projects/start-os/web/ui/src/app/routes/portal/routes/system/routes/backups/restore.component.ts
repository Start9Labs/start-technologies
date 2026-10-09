import { DatePipe } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import {
  DialogService,
  getErrorMessage,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiInput, TuiTitle } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { BackupContext } from './backup.types'
import { RECOVER } from './recover.component'

@Component({
  template: `
    @if (!serverId) {
      <table [appTable]="['Hostname', 'StartOS Version', 'Created', null]">
        @for (server of servers; track server[0]) {
          <tr>
            <td class="name">{{ server[1].hostname }}.local</td>
            <td>{{ server[1].version }}</td>
            <td>{{ server[1].timestamp | date: 'medium' }}</td>
            <td>
              <button tuiButton size="s" (click)="serverId = server[0]">
                {{ 'Select' | i18n }}
              </button>
            </td>
          </tr>
        }
      </table>
    } @else {
      <section class="unlock-flow">
        <span tuiTitle>
          <b>{{ 'Password' | i18n }}</b>
          <span tuiSubtitle>
            {{ selectedServerName() }} ·
            {{
              'Enter the master password that was used to encrypt this backup.'
                | i18n
            }}
          </span>
        </span>
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input
            tuiInput
            required
            autocomplete="current-password"
            [type]="passwordMasked ? 'password' : 'text'"
            [(ngModel)]="password"
            (keyup.enter)="decrypt()"
          />
          <button
            tuiIconButton
            type="button"
            size="xs"
            appearance="icon"
            [iconStart]="passwordMasked ? '@tui.eye' : '@tui.eye-off'"
            [attr.aria-label]="
              (passwordMasked ? 'Show password' : 'Hide password') | i18n
            "
            (click)="passwordMasked = !passwordMasked"
          >
            {{ (passwordMasked ? 'Show password' : 'Hide password') | i18n }}
          </button>
        </tui-textfield>
        <footer class="g-buttons">
          @if (servers.length > 1) {
            <button
              tuiButton
              type="button"
              appearance="flat"
              (click)="serverId = ''"
            >
              {{ 'Back' | i18n }}
            </button>
          }
          <button tuiButton [disabled]="!password()" (click)="decrypt()">
            {{ 'Continue' | i18n }}
          </button>
        </footer>
      </section>
    }
  `,
  styles: `
    td:last-child {
      text-align: end;
    }

    .unlock-flow {
      display: grid;
      gap: 1rem;
    }

    :host-context(tui-root._mobile) {
      tr {
        grid-template-columns: 1fr auto;
      }

      .name {
        color: var(--tui-text-primary);
        font: var(--tui-typography-body-m);
        font-weight: bold;
      }

      td:last-child {
        grid-area: 1 / 2 / 4 / 2;
        align-self: center;
      }
    }
  `,
  imports: [
    DatePipe,
    FormsModule,
    TuiButton,
    TuiInput,
    TuiTitle,
    TableComponent,
    i18nPipe,
  ],
})
export class BackupRestoreComponent {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly context = injectContext<BackupContext>()
  private readonly i18n = inject(i18nPipe)

  protected readonly target = this.context.data
  protected readonly servers = this.serverEntries()
  protected serverId = this.servers.length === 1 ? this.servers[0]![0] : ''
  protected readonly password = signal('')
  protected passwordMasked = true

  protected selectedServerName(): string {
    const server = this.servers.find(([id]) => id === this.serverId)?.[1]
    return server ? `${server.hostname}.local` : this.serverId
  }

  protected async decrypt() {
    const password = this.password()
    if (!this.serverId || !password) return
    await this.tasks.run(async () => {
      const params = {
        targetId: this.target.id,
        serverId: this.serverId,
        password,
      }
      const [manual, automatic] = await Promise.allSettled([
        this.api.getBackupInfo(params),
        this.api.discoverScheduledBackupHistories(params),
      ])

      if (manual.status === 'rejected' && automatic.status === 'rejected') {
        throw manual.reason
      }

      const backupInfo =
        manual.status === 'fulfilled'
          ? manual.value
          : { version: '', timestamp: null, packageBackups: {} }
      const scheduledHistories =
        automatic.status === 'fulfilled' ? automatic.value : []

      if (
        !Object.keys(backupInfo.packageBackups).length &&
        !scheduledHistories.length
      ) {
        throw new Error(
          this.i18n.transform('No restorable checkpoints were found'),
        )
      }

      const data = {
        targetId: this.target.id,
        serverId: this.serverId,
        backupInfo,
        scheduledHistories,
        password,
        unavailableSources: [
          manual.status === 'rejected' && this.hasSource(false)
            ? `${this.i18n.transform('Manual backups could not be opened')}: ${getErrorMessage(manual.reason)}`
            : '',
          automatic.status === 'rejected' && this.hasSource(true)
            ? `${this.i18n.transform('Automatic backups could not be opened')}: ${getErrorMessage(automatic.reason)}`
            : '',
        ].filter(Boolean),
      }

      this.context.$implicit.complete()
      this.dialog
        .openComponent(RECOVER, { label: 'Select services', data })
        .subscribe()
    }, 'Decrypting drive')
  }

  private hasSource(scheduled: boolean): boolean {
    return Object.entries(this.target.entry.startOs).some(
      ([key, server]) =>
        (server.serverId || key) === this.serverId &&
        !!server.scheduled === scheduled,
    )
  }

  private serverEntries(): [string, T.StartOsRecoveryInfo][] {
    const servers = new Map<string, T.StartOsRecoveryInfo>()
    for (const [key, server] of Object.entries(this.target.entry.startOs)) {
      const id = server.serverId || key
      const current = servers.get(id)
      if (!current || current.timestamp < server.timestamp) {
        servers.set(id, server)
      }
    }
    return [...servers]
  }
}

export const BACKUP_RESTORE = new PolymorpheusComponent(BackupRestoreComponent)
