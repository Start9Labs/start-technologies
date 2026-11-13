import { Component, computed, inject, signal } from '@angular/core'
import { RouterLink } from '@angular/router'
import { WA_WINDOW } from '@ng-web-apis/common'
import {
  DialogService,
  getRpcErrorDetails,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { Version } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { TuiProgress } from '@taiga-ui/kit'
import { filter } from 'rxjs'
import { StartupApiService } from 'src/app/services/api/startup-api.service'
import { ConfigService } from 'src/app/services/config.service'
import { DiagnosticUpdate } from './update.service'

@Component({
  template: `
    @if (!restarted()) {
      <h1 class="title">StartOS - {{ 'Diagnostic Mode' | i18n }}</h1>
      @if (error(); as error) {
        <h2 class="subtitle">StartOS {{ 'launch error' | i18n }}:</h2>
        <code class="code warning">
          <p>{{ error.problem | i18n }}</p>
          @if (error.details) {
            <p>{{ error.details }}</p>
          }
        </code>
        <a tuiButton routerLink="logs">{{ 'View logs' | i18n }}</a>
        <h2 class="subtitle">{{ 'Possible solutions' | i18n }}:</h2>
        <code class="code">
          <p>
            {{
              (error.solution | i18n)
                .replace('{restartAction}', actions.restart | i18n)
                .replace('{setupAction}', actions.setup | i18n)
                .replace('{recoveryAction}', actions.recovery | i18n)
            }}
          </p>
        </code>
        <div class="buttons">
          <button tuiButton (click)="restart()">
            {{ actions.restart | i18n }}
          </button>
          @if (error.code === 15 || error.code === 25) {
            <button tuiButton appearance="secondary" (click)="forgetDrive()">
              {{
                (error.code === 15 ? actions.setup : actions.recovery) | i18n
              }}
            </button>
          }
          <button
            tuiButton
            appearance="secondary"
            [disabled]="!!updateState.progress() || updateState.updated()"
            (click)="updateState.update()"
          >
            {{ 'Update server' | i18n }}
          </button>
          <button
            tuiButton
            appearance="secondary-destructive"
            (click)="presentAlertRepairDisk()"
          >
            {{ 'Repair drive' | i18n }}
          </button>
        </div>
        @if (updateState.progress(); as progress) {
          <h2 class="subtitle">
            {{ (reinstall() ? 'Reinstalling' : 'Updating') | i18n }}
            StartOS {{ updateState.target() }}:
            {{ (progress.total * 100).toFixed(0) }}%
          </h2>
          <progress
            tuiProgressBar
            [max]="1"
            [value]="progress.total"
          ></progress>
          <p [innerHTML]="progress.message"></p>
        } @else if (updateState.updated()) {
          <h2 class="subtitle">
            StartOS {{ updateState.target() }}:
            {{ 'Download complete. Restart to apply.' | i18n }}
          </h2>
        }
      }
    } @else {
      <h1 class="title">{{ 'Server is restarting' | i18n }}</h1>
      <h2 class="subtitle">
        {{ 'Wait for the server to restart, then refresh this page.' | i18n }}
      </h2>
      <button tuiButton (click)="refreshPage()">{{ 'Refresh' | i18n }}</button>
    }
  `,
  styles: `
    :host {
      display: block;
      padding: 2rem;
      overflow: auto;
      background: var(--tui-background-base);
    }

    .title {
      text-align: center;
      padding-bottom: 1.5rem;
      font-size: calc(2vw + 1rem);
    }

    .subtitle {
      padding-top: 1rem;
      padding-bottom: 1rem;
      font-size: calc(1vw + 0.75rem);
      font-weight: bold;
    }

    .code {
      display: block;
      color: var(--tui-status-positive);
      background: rgb(69, 69, 69);
      padding: 1px 1rem;
      margin-bottom: 2rem;
    }

    .warning {
      color: var(--tui-status-warning);
    }

    .buttons {
      display: flex;
      flex-wrap: wrap;
      gap: 1rem;
    }
  `,
  providers: [DiagnosticUpdate],
  imports: [TuiButton, TuiProgress, i18nPipe, RouterLink],
})
export default class HomePage {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(StartupApiService)
  private readonly dialog = inject(DialogService)
  private readonly window = inject(WA_WINDOW)

  private readonly config = inject(ConfigService)
  protected readonly updateState = inject(DiagnosticUpdate)
  protected readonly actions = {
    restart: 'Restart server',
    setup: 'Setup current drive',
    recovery: 'Enter recovery mode',
  } as const satisfies Record<string, i18nKey>
  protected readonly restarted = signal(false)
  protected readonly reinstall = computed(() => {
    const target = this.updateState.target()
    return (
      !!target &&
      Version.parse(target).compare(Version.parse(this.config.version)) ===
        'equal'
    )
  })
  protected readonly error = signal<
    | {
        code: number
        problem: i18nKey
        solution: i18nKey
        details?: string
      }
    | undefined
  >(undefined)

  constructor() {
    void this.loadError()
  }

  private async loadError() {
    try {
      const error = await this.api.rpc.diagnostic.error({})
      if (error.code === 15) {
        this.error.set({
          code: 15,
          problem: 'Unknown storage drive detected',
          solution:
            'To use a different storage drive, replace the current one and click {restartAction} below. To use the current storage drive, click {setupAction} below, then follow instructions. No data will be erased during this process.',
          details: getRpcErrorDetails(error.data),
        })
      } else if (error.code === 20) {
        this.error.set({
          code: 20,
          problem: 'Storage drive not found',
          solution:
            'Insert your StartOS storage drive and click {restartAction} below.',
          details: getRpcErrorDetails(error.data),
        })
      } else if (error.code === 25) {
        this.error.set({
          code: 25,
          problem:
            'Storage drive corrupted. This could be the result of data corruption or physical damage.',
          solution:
            'It may or may not be possible to re-use this drive by reformatting and recovering from backup. To enter recovery mode, click {recoveryAction} below, then follow instructions. No data will be erased during this step.',
          details: getRpcErrorDetails(error.data),
        })
      } else if (error.code === 2) {
        this.error.set({
          code: 2,
          problem: 'Filesystem error',
          solution:
            'Repairing the disk could help resolve this issue. Please DO NOT unplug the drive or server during this time or the situation will become worse.',
          details: getRpcErrorDetails(error.data),
        })
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
      await this.api.rpc.diagnostic.restart({})
      this.restarted.set(true)
    }, 'Loading')
  }

  forgetDrive() {
    this.tasks.run(async () => {
      await this.api.rpc.diagnostic.disk.forget({})
      await this.api.rpc.diagnostic.restart({})
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
      await this.api.rpc.diagnostic.disk.repair({})
      await this.api.rpc.diagnostic.restart({})
      this.restarted.set(true)
    }, 'Loading')
  }
}
