import { Component, inject, signal } from '@angular/core'
import { Router } from '@angular/router'
import { DialogService, ErrorService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiLink,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiChevron } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from '../services/api.service'
import { StateService } from '../services/state.service'
import { StartOSDiskInfoFull, StartOSDiskInfoWithId } from '../types'
import { CIFS, CifsResult } from '../components/cifs.component'
import { SELECT_NETWORK_BACKUP } from '../components/select-network-backup.dialog'
import { UNLOCK_PASSWORD } from '../components/unlock-password.dialog'

@Component({
  template: `
    <section tuiCardLarge="compact">
      <header tuiHeader>
        <hgroup tuiTitle>
          <h2>{{ 'Select Backup' | i18n }}</h2>
          <p tuiSubtitle>
            {{ 'Select the StartOS backup you want to restore' | i18n }}
            <button
              tuiLink
              appearance="action"
              iconEnd="@tui.rotate-cw"
              [textContent]="'Refresh' | i18n"
              (click)="refresh()"
            ></button>
          </p>
        </hgroup>
      </header>

      @if (loading()) {
        <tui-loader />
      } @else {
        <button
          tuiButton
          tuiChevron
          tuiDropdown
          tuiDropdownLimitWidth="fixed"
          [(tuiDropdownOpen)]="open"
        >
          {{ 'Select Backup' | i18n }}
          <tui-data-list *tuiDropdown>
            <button tuiOption iconStart="@tui.folder-plus" (click)="openCifs()">
              {{ 'Open Network Backup' | i18n }}
            </button>
            <hr />
            <tui-opt-group [label]="'Physical Backups' | i18n">
              @for (server of physicalServers(); track server.id) {
                <button tuiOption (click)="selectPhysicalBackup(server)">
                  <span tuiTitle>
                    {{ server.hostname }}
                    <span tuiSubtitle>
                      {{
                        (server.scheduled
                          ? 'Automatic backup'
                          : 'Manual backup'
                        ) | i18n
                      }}
                      · {{ server.drive.vendor }} {{ server.drive.model }} ·
                      {{ server.partition.logicalname }}
                    </span>
                  </span>
                </button>
              } @empty {
                <button tuiOption [disabled]="true">
                  {{ 'No physical backups' | i18n }}
                </button>
              }
            </tui-opt-group>
          </tui-data-list>
        </button>
      }
    </section>
  `,
  imports: [
    TuiButton,
    TuiCardLarge,
    TuiDataList,
    TuiDropdown,
    TuiLoader,
    TuiTitle,
    TuiHeader,
    i18nPipe,
    TuiLink,
    TuiChevron,
  ],
})
export default class RestorePage {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly dialogs = inject(DialogService)
  private readonly errorService = inject(ErrorService)
  private readonly stateService = inject(StateService)

  readonly loading = signal(true)
  open = false
  readonly physicalServers = signal<StartOSDiskInfoFull[]>([])

  async ngOnInit() {
    await this.loadDrives()
  }

  async refresh() {
    this.loading.set(true)
    await this.loadDrives()
  }

  openCifs() {
    this.open = false
    this.dialogs
      .openComponent<CifsResult>(CIFS, { label: 'Connect Network Folder' })
      .subscribe(result => {
        if (result) {
          this.handleCifsResult(result)
        }
      })
  }

  selectPhysicalBackup(server: StartOSDiskInfoFull) {
    this.open = false
    this.showUnlockDialog(
      server.serverId || server.id,
      {
        type: 'disk',
        logicalname: server.partition.logicalname,
      },
      !!server.scheduled,
    )
  }

  private handleCifsResult(result: CifsResult) {
    if (result.servers.length === 1) {
      const server = result.servers[0]!
      this.showUnlockDialog(
        server.serverId || server.id,
        {
          type: 'cifs',
          ...result.cifs,
        },
        !!server.scheduled,
      )
    } else {
      this.showSelectNetworkBackupDialog(result.cifs, result.servers)
    }
  }

  private showSelectNetworkBackupDialog(
    cifs: T.Cifs,
    servers: StartOSDiskInfoWithId[],
  ) {
    this.dialogs
      .openComponent<StartOSDiskInfoWithId | null>(SELECT_NETWORK_BACKUP, {
        data: { servers },
      })
      .subscribe(server => {
        if (server) {
          this.showUnlockDialog(
            server.serverId || server.id,
            { type: 'cifs', ...cifs },
            !!server.scheduled,
          )
        }
      })
  }

  private showUnlockDialog(
    serverId: string,
    target: { type: 'disk'; logicalname: string } | ({ type: 'cifs' } & T.Cifs),
    scheduled: boolean,
  ) {
    this.dialogs
      .openComponent<string | null>(UNLOCK_PASSWORD)
      .subscribe(password => {
        if (password) {
          this.stateService.recoverySource = {
            type: 'backup',
            target,
            serverId,
            password,
            scheduled,
          }
          this.router.navigate(['/password'])
        }
      })
  }

  private async loadDrives() {
    this.physicalServers.set([])

    try {
      const drives = await this.api.getDisks()

      this.physicalServers.set(
        drives.flatMap(drive =>
          drive.partitions.flatMap(partition =>
            Object.entries(partition.startOs).map(([id, val]) => ({
              id,
              ...val,
              partition,
              drive,
            })),
          ),
        ),
      )
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}
