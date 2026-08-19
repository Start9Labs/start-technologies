import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { DialogService, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { defer, filter, map, Observable, switchMap } from 'rxjs'
import { POWER } from 'src/app/routes/portal/components/header/power.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'

@Injectable({ providedIn: 'root' })
export class PowerService {
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  readonly backingUp = toSignal(inject(OSService).backingUp$, {
    initialValue: false,
  })

  /**
   * Every in-app route to a restart or shutdown goes through here, so that none
   * of them can interrupt a backup: during one the user is offered the choice
   * of waiting for it, and the server keeps whichever choice is made. Emits
   * once the server has been asked, `true` if the user chose to wait — not what
   * the server then did, which the caller cannot see. A dismissed prompt asks
   * for nothing and so emits nothing.
   */
  power(action: T.PowerAction): Observable<boolean> {
    if (!this.backingUp()) return this.run(action, false)

    return this.dialog
      .openComponent<boolean>(POWER, {
        label: action === 'restart' ? 'Restart' : 'Warning',
        size: 's',
        data: action,
      })
      .pipe(switchMap(now => this.run(action, !now)))
  }

  cancel() {
    this.tasks.run(async () => await this.api.cancelDeferredPower({}))
  }

  private run(
    action: T.PowerAction,
    afterBackup: boolean,
  ): Observable<boolean> {
    return defer(() =>
      this.tasks.run(
        async () =>
          action === 'restart'
            ? await this.api.restartServer({ afterBackup })
            : await this.api.shutdownServer({ afterBackup }),
        afterBackup ? 'Wait for backup to complete' : `Beginning ${action}`,
      ),
    ).pipe(
      filter(Boolean),
      map(() => afterBackup),
    )
  }
}
