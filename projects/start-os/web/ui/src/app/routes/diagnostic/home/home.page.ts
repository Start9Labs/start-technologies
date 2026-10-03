import { CommonModule } from '@angular/common'
import { Component, inject, OnInit, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import { WA_WINDOW } from '@ng-web-apis/common'
import {
  DialogService,
  getRpcErrorDetails,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { filter } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ConfigService } from 'src/app/services/config.service'

@Component({
  selector: 'diagnostic-home',
  templateUrl: 'home.component.html',
  styleUrls: ['home.page.scss'],
  imports: [CommonModule, TuiButton, i18nPipe, RouterLink],
})
export default class HomePage implements OnInit {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)
  private readonly window = inject(WA_WINDOW)

  readonly config = inject(ConfigService)
  readonly restarted = signal(false)
  readonly error = signal<
    | {
        code: number
        problem: i18nKey
        solution: i18nKey
        details?: string
      }
    | undefined
  >(undefined)

  async ngOnInit() {
    try {
      const error = await this.api.diagnosticGetError()
      // incorrect drive
      if (error.code === 15) {
        this.error.set({
          code: 15,
          problem: 'Unknown storage drive detected',
          solution:
            'To use a different storage drive, replace the current one and click RESTART SERVER below. To use the current storage drive, click USE CURRENT DRIVE below, then follow instructions. No data will be erased during this process.',
          details: getRpcErrorDetails(error.data),
        })
        // no drive
      } else if (error.code === 20) {
        this.error.set({
          code: 20,
          problem: 'Storage drive not found',
          solution:
            'Insert your StartOS storage drive and click RESTART SERVER below.',
          details: getRpcErrorDetails(error.data),
        })
        // drive corrupted
      } else if (error.code === 25) {
        this.error.set({
          code: 25,
          problem:
            'Storage drive corrupted. This could be the result of data corruption or physical damage.',
          solution:
            'It may or may not be possible to re-use this drive by reformatting and recovering from backup. To enter recovery mode, click ENTER RECOVERY MODE below, then follow instructions. No data will be erased during this step.',
          details: getRpcErrorDetails(error.data),
        })
        // filesystem I/O error - disk needs repair
      } else if (error.code === 2) {
        this.error.set({
          code: 2,
          problem: 'Filesystem error',
          solution:
            'Repairing the disk could help resolve this issue. Please DO NOT unplug the drive or server during this time or the situation will become worse.',
          details: getRpcErrorDetails(error.data),
        })
        // disk management error - disk needs repair
      } else if (error.code === 48) {
        this.error.set({
          code: 48,
          problem: 'Disk management error',
          solution:
            'Repairing the disk could help resolve this issue. Please DO NOT unplug the drive or server during this time or the situation will become worse.',
          details: getRpcErrorDetails(error.data),
        })
      } else {
        this.error.set({
          code: error.code,
          problem: error.message as i18nKey,
          solution: 'Please contact support',
          details: getRpcErrorDetails(error.data),
        })
      }
    } catch (e) {
      console.error(e)
    }
  }

  restart() {
    this.tasks.run(async () => {
      await this.api.diagnosticRestart()
      this.restarted.set(true)
    }, 'Loading')
  }

  forgetDrive() {
    this.tasks.run(async () => {
      await this.api.diagnosticForgetDrive()
      await this.api.diagnosticRestart()
      this.restarted.set(true)
    }, 'Loading')
  }

  async presentAlertRepairDisk() {
    this.dialog
      .openConfirm({
        label: 'Warning',
        size: 's',
        data: {
          no: 'Cancel',
          yes: 'Repair',
          content:
            'This action should only be executed if directed by a Start9 support specialist. We recommend backing up your device before preforming this action. If anything happens to the device during the reboot, such as losing power or unplugging the drive, the filesystem will be in an unrecoverable state. Please proceed with caution.',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.repairDisk())
  }

  refreshPage(): void {
    this.window.location.reload()
  }

  private repairDisk() {
    this.tasks.run(async () => {
      await this.api.diagnosticRepairDisk()
      await this.api.diagnosticRestart()
      this.restarted.set(true)
    }, 'Loading')
  }
}
