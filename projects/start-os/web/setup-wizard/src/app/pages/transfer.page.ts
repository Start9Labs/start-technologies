import { Component, inject, OnInit, signal } from '@angular/core'
import { Router } from '@angular/router'
import {
  DialogService,
  DiskInfo,
  ErrorService,
  i18nPipe,
  toGuid,
} from '@start9labs/shared'
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
import { filter } from 'rxjs'
import { ApiService } from '../services/api.service'
import { StateService } from '../services/state.service'

@Component({
  template: `
    <section tuiCardLarge="compact">
      <header tuiHeader>
        <hgroup tuiTitle>
          <h2>{{ 'Transfer Data' | i18n }}</h2>
          <p tuiSubtitle>
            {{
              'Select the drive containing your existing StartOS data' | i18n
            }}
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
          {{ 'Select Drive' | i18n }}
          <tui-data-list
            *tuiDropdown
            [emptyContent]="'No StartOS data drives found' | i18n"
          >
            @for (drive of drives(); track drive.logicalname) {
              <button tuiOption (click)="select(drive)">
                <span tuiTitle>
                  {{ drive.vendor }} {{ drive.model }}
                  <span tuiSubtitle>{{ drive.logicalname }}</span>
                </span>
              </button>
            }
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
    TuiLink,
    TuiChevron,
    TuiLoader,
    TuiTitle,
    TuiHeader,
    i18nPipe,
  ],
})
export default class TransferPage implements OnInit {
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly dialogs = inject(DialogService)
  private readonly errorService = inject(ErrorService)
  private readonly stateService = inject(StateService)

  readonly loading = signal(true)
  open = false
  readonly drives = signal<DiskInfo[]>([])

  async ngOnInit() {
    await this.loadDrives()
  }

  async refresh() {
    this.loading.set(true)
    await this.loadDrives()
  }

  select(drive: DiskInfo) {
    this.open = false

    this.dialogs
      .openConfirm({
        label: 'Warning',
        data: {
          content:
            'After transferring data from this drive, do not attempt to boot into it again as a Start9 Server. This may result in services malfunctioning, data corruption, or loss of funds.',
          yes: 'Continue',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        const guid = toGuid(drive)
        if (guid) {
          this.stateService.recoverySource = {
            type: 'migrate',
            guid,
          }
          this.router.navigate(['/password'])
        }
      })
  }

  private async loadDrives() {
    try {
      const allDrives = await this.api.rpc.setup.disk.list({})
      // Filter to only drives with StartOS data (guid)
      this.drives.set(allDrives.filter(toGuid))
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}
